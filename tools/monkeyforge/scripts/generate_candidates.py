"""Generate a local preference-dataset candidate set with no GPU.

Builds procedural accessory variants spanning clean to broken, compiles each onto a base monkey,
measures it, renders it, and writes the `candidates.jsonl` that `review_pairs.py plan` consumes.

    python scripts/generate_candidates.py --output-dir output/candidates

Then:

    python scripts/review_pairs.py plan --candidates output/candidates/candidates.jsonl \
        --output output/review/pairs.csv
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY_ROOT / "src"))

from monkeyforge.config import Settings  # noqa: E402
from monkeyforge.features import extract_features  # noqa: E402
from monkeyforge.measurements import load_measurement  # noqa: E402
from monkeyforge.models import (  # noqa: E402
    SOCKET_NODE_NAMES,
    AccessorySpec,
    Archetype,
    Socket,
)
from monkeyforge.variants import Defect, build_variant, default_catalogue  # noqa: E402

KIND_SOCKETS: dict[str, Socket] = {
    "hat": Socket.HEAD_TOP,
    "backpack": Socket.BACK,
    "shield": Socket.HAND_LEFT,
}

KIND_ARCHETYPES: dict[str, Archetype] = {
    "hat": Archetype.GOALKEEPER,
    "backpack": Archetype.RUNNER,
    "shield": Archetype.DEFENDER,
}

PART_SCHEMAS: dict[str, list[str]] = {
    "hat": ["shell", "crest"],
    "backpack": ["body"],
    "shield": ["plate"],
}


def slugify(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")[:48]


def repo_relative(path: Path) -> str:
    """Path relative to the repository root, so the manifest survives being moved or shared."""
    try:
        return path.resolve().relative_to(REPOSITORY_ROOT).as_posix()
    except ValueError:
        return path.resolve().as_posix()


def write_obj(path: Path, vertices, triangles, name: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"# MonkeyForge procedural variant {name}", f"o {name}"]
    lines += [f"v {x:.6f} {y:.6f} {z:.6f}" for x, y, z in vertices]
    # OBJ indices are 1-based.
    lines += [f"f {a + 1} {b + 1} {c + 1}" for a, b, c in triangles]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_blender(blender: str, script: Path, args: list[str], label: str) -> bool:
    command = [blender, "--background", "--python", str(script), "--", *args]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        tail = (result.stdout or "")[-600:]
        print(f"      FAILED {label}: exit {result.returncode}")
        print(f"      {tail.strip()[:400]}")
        return False
    return True


def base_model_for(settings: Settings, archetype: Archetype) -> Path:
    mapping = {
        Archetype.BALANCED: settings.base_model,
        Archetype.PLAYMAKER: settings.base_model,
        Archetype.RUNNER: settings.runner_base_model,
        Archetype.DEFENDER: settings.defender_base_model,
        Archetype.GOALKEEPER: settings.goalkeeper_base_model,
    }
    return mapping.get(archetype, settings.base_model)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=Path("output/candidates"))
    parser.add_argument("--resolution", type=int, default=384)
    parser.add_argument("--target-triangles", type=int, default=2500)
    parser.add_argument("--limit", type=int, default=0, help="stop after N candidates (0 = all)")
    parser.add_argument("--skip-render", action="store_true")
    args = parser.parse_args()

    settings = Settings()
    blender = settings.blender_path
    compile_script = REPOSITORY_ROOT / "scripts" / "blender_compile.py"
    measure_script = REPOSITORY_ROOT / "scripts" / "blender_measure_glb.py"
    render_script = REPOSITORY_ROOT / "scripts" / "blender_render_preview.py"

    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = output_dir / "candidates.jsonl"

    catalogue = default_catalogue()
    total = sum(len(variants) for _, _, variants in catalogue)
    if args.limit:
        total = min(total, args.limit)

    print(f"Generating {total} candidates into {output_dir}\n")
    records: list[dict] = []
    produced = 0
    started = time.monotonic()

    for prompt, kind, variants in catalogue:
        socket = KIND_SOCKETS[kind]
        archetype = KIND_ARCHETYPES[kind]
        base_model = base_model_for(settings, archetype)
        if not base_model.exists():
            print(f"SKIP {prompt!r}: base model missing ({base_model})")
            continue

        accessory = AccessorySpec(
            id="primary_accessory",
            slot=socket,
            kind=kind,
            description=prompt,
            target_triangles=args.target_triangles,
            part_schema=PART_SCHEMAS[kind],
        )
        prompt_slug = slugify(prompt)

        for variant in variants:
            if args.limit and produced >= args.limit:
                break
            produced += 1
            work = output_dir / prompt_slug / variant.variant_id
            work.mkdir(parents=True, exist_ok=True)
            print(f"[{produced}/{total}] {prompt_slug}/{variant.variant_id} ({variant.defect})")

            vertices, triangles = build_variant(variant)
            obj_path = work / "accessory.obj"
            write_obj(obj_path, vertices, triangles, variant.variant_id)
            print(f"      built {len(vertices)} verts / {len(triangles)} tris")

            glb_path = work / "character.glb"
            if not run_blender(
                blender,
                compile_script,
                [
                    "--base", str(base_model),
                    "--accessory", str(obj_path),
                    "--output", str(glb_path),
                    "--socket", SOCKET_NODE_NAMES[socket],
                    "--target-triangles", str(args.target_triangles),
                ],
                "compile",
            ):
                continue

            # Measure the RAW accessory, not the compiled character. The compiler re-seats the
            # prop onto its socket and decimates it to budget, so socket fit and triangle count
            # measured after compilation are constant by construction and tell you nothing about
            # what the generator produced. The compiled GLB is still what gets rendered.
            measurement_path = work / "measurement.json"
            run_blender(
                blender,
                measure_script,
                [
                    "--input", str(obj_path),
                    "--output", str(measurement_path),
                    "--bare",
                ],
                "measure",
            )
            if not measurement_path.exists():
                print("      FAILED measure: no document produced")
                continue

            measurement = load_measurement(measurement_path)
            features = extract_features(measurement=measurement, accessory=accessory)
            (work / "features.json").write_text(
                json.dumps(features.model_dump(mode="json"), indent=2), encoding="utf-8"
            )

            attached = measurement.accessory
            print(
                f"      tris={attached.triangles if attached else 0} "
                f"parts={attached.loose_parts if attached else 0} "
                f"fit={features.socket_fit_score:.2f} "
                f"budget={features.triangle_budget_score:.2f} "
                f"broken={features.disconnected_penalty:.2f}"
            )

            render_path = work / "render.png"
            if not args.skip_render:
                run_blender(
                    blender,
                    render_script,
                    [
                        "--glb", str(glb_path),
                        "--output", str(render_path),
                        "--resolution", str(args.resolution),
                    ],
                    "render",
                )

            records.append(
                {
                    "id": f"{prompt_slug}/{variant.variant_id}",
                    "prompt": prompt,
                    "provenance": "procedural",
                    "render": repo_relative(render_path) if render_path.exists() else "",
                    "features": repo_relative(work / "features.json"),
                    "defect": str(variant.defect),
                    "notes": variant.notes,
                }
            )

    with manifest_path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")

    elapsed = time.monotonic() - started
    clean = sum(1 for record in records if record["defect"] == Defect.NONE)
    print(f"\nWrote {len(records)} candidates ({clean} clean, {len(records) - clean} defective)")
    print(f"  manifest: {manifest_path}")
    print(f"  elapsed:  {elapsed:.0f}s")
    print("\nNext:")
    print(f"  python scripts/review_pairs.py plan --candidates {manifest_path} \\")
    print("      --output output/review/pairs.csv")


if __name__ == "__main__":
    main()
