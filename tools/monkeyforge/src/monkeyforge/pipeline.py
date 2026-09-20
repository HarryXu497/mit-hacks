from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Protocol

from monkeyforge.compilers import AssetCompiler
from monkeyforge.features import extract_features
from monkeyforge.measurements import GlbMeasurement, MeasurementError
from monkeyforge.models import AccessorySpec, BuildIdentity, CompiledAsset, JobState
from monkeyforge.providers import GeometryProvider
from monkeyforge.spec_provider import SpecProvider
from monkeyforge.storage import JobStore
from monkeyforge.validation import validate_compiled_asset


class AssetMeasurer(Protocol):
    async def measure(
        self,
        compiled: CompiledAsset,
        accessory: AccessorySpec,
        output_dir: Path,
    ) -> GlbMeasurement: ...


def request_digest(
    description: str,
    seed: int,
    sketch_bytes: bytes,
    build: BuildIdentity | None = None,
) -> str:
    """Content-address a build: same inputs *and* same toolchain -> same job id.

    `build` is part of the key, not decoration. Without it a generator or compiler
    swap silently reuses cached output from the previous toolchain — the job id is
    unchanged, so `api.create_character` returns the stale job and never rebuilds.
    It stays optional so callers that only want to address the request (tests,
    tooling) need not construct an identity.
    """
    digest = hashlib.sha256()
    digest.update(b"monkeyforge-v1\0")
    digest.update(description.strip().encode("utf-8"))
    digest.update(b"\0")
    digest.update(str(seed).encode("ascii"))
    digest.update(b"\0")
    digest.update(sketch_bytes)
    if build is not None:
        digest.update(b"\0")
        digest.update(build.digest().encode("ascii"))
    return digest.hexdigest()[:20]


class CharacterPipeline:
    def __init__(
        self,
        store: JobStore,
        spec_provider: SpecProvider,
        geometry_provider: GeometryProvider,
        compiler: AssetCompiler,
        measurer: AssetMeasurer | None = None,
        build: BuildIdentity | None = None,
    ) -> None:
        self.store = store
        self.spec_provider = spec_provider
        self.geometry_provider = geometry_provider
        self.compiler = compiler
        self.measurer = measurer
        self.build = build

    async def run(self, job_id: str) -> None:
        record = self.store.get(job_id)
        if record is None:
            raise KeyError(job_id)

        try:
            self.store.transition(job_id, JobState.PARSING)
            spec = await self.spec_provider.create_spec(record.request, str(record.sketch_path))
            self.store.update(job_id, spec=spec)

            if not spec.accessories:
                raise ValueError("CharacterSpec contains no accessories")
            accessory = spec.accessories[0]
            job_dir = self.store.job_dir(job_id)

            self.store.transition(job_id, JobState.GENERATING)
            generated = await self.geometry_provider.generate(
                accessory=accessory,
                reference_path=record.sketch_path,
                seed=record.request.seed,
                output_dir=job_dir / "generated",
            )

            self.store.transition(job_id, JobState.COMPILING)
            compiled = await self.compiler.compile(
                generated=generated,
                accessory=accessory,
                output_dir=job_dir / "output",
                archetype=spec.archetype,
                palette=spec.palette,
            )

            self.store.transition(job_id, JobState.VALIDATING)
            output_dir = job_dir / "output"

            measurement: GlbMeasurement | None = None
            if self.measurer is not None:
                try:
                    measurement = await self.measurer.measure(compiled, accessory, output_dir)
                except MeasurementError as exc:
                    # A failed measurement must not be mistaken for a clean asset; fall through to
                    # file-size-only validation, which reports itself as unmeasured.
                    measurement = None
                    measurement_error = str(exc)
                else:
                    measurement_error = None
            else:
                measurement_error = None

            metrics = validate_compiled_asset(compiled, accessory, measurement)
            if measurement_error is not None:
                metrics.warnings.append(f"measurement pass failed: {measurement_error}")

            if measurement is not None:
                features = extract_features(
                    measurement=measurement,
                    accessory=accessory,
                    palette=spec.palette,
                )
                (output_dir / "features.json").write_text(
                    json.dumps(features.model_dump(mode="json"), indent=2), encoding="utf-8"
                )

            # The lockfile half of the manifest: everything needed to decide whether
            # this output can be reused, and to reproduce it if it cannot.
            build = self.build or BuildIdentity(
                provider=generated.provider, compiler=compiled.compiler
            )
            manifest = {
                "schema_version": "1.0",
                "job_id": job_id,
                "character": spec.model_dump(mode="json"),
                "asset": {
                    "path": compiled.path.name,
                    "media_type": compiled.media_type,
                    "provider": generated.provider,
                    "compiler": compiled.compiler,
                    "base_profile": spec.archetype.value,
                },
                "build": {
                    **build.model_dump(mode="json"),
                    "digest": build.digest(),
                    # Ground truth from the generator, which disagrees with
                    # build.provider_version when the orchestrator is misconfigured.
                    "observed_provider_version": generated.provider_version,
                    "observed_compiler_version": compiled.compiler_version,
                },
            }
            (output_dir / "manifest.json").write_text(
                json.dumps(manifest, indent=2), encoding="utf-8"
            )
            (output_dir / "metrics.json").write_text(
                json.dumps(metrics.model_dump(mode="json"), indent=2), encoding="utf-8"
            )

            if not metrics.valid:
                raise ValueError("compiled asset failed validation")
            self.store.update(
                job_id,
                state=JobState.COMPLETE,
                output_path=compiled.path,
                metrics=metrics,
                build=build,
                observed_provider_version=generated.provider_version,
            )
        except Exception as exc:
            self.store.update(job_id, state=JobState.FAILED, error=str(exc))
