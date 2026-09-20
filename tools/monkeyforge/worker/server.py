"""MonkeyForge GPU worker.

Implements `docs/gpu-worker-contract.md`: one multipart POST in, one binary GLB out. The generator
behind it is pluggable so the network path can be validated with no GPU before a real image-to-3D
model is installed.

    MONKEYFORGE_WORKER_GENERATOR=stub    procedural GLB, no GPU, no weights
    MONKEYFORGE_WORKER_GENERATOR=trellis TRELLIS.2

Run:
    uvicorn server:app --host 0.0.0.0 --port 8600
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
import time
from pathlib import Path

from fastapi import FastAPI, Form, Header, HTTPException, Response, UploadFile
from fastapi import File as FileField
from generators import load_generator

LOGGER = logging.getLogger("monkeyforge.worker")
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

CONTRACT_VERSION = "1"
MAX_REFERENCE_BYTES = 10 * 1024 * 1024

TOKEN = os.environ.get("MONKEYFORGE_WORKER_TOKEN") or None
GENERATOR_NAME = os.environ.get("MONKEYFORGE_WORKER_GENERATOR", "stub")
CACHE_DIR = Path(os.environ.get("MONKEYFORGE_WORKER_CACHE", "cache"))

app = FastAPI(title="MonkeyForge worker", version="0.1.0")
generator = load_generator(GENERATOR_NAME)


#: Why the generator is not ready, when it failed to warm. Surfaced by /health so an operator can
#: see the cause without reading logs -- a gated model download, for instance, is a configuration
#: problem a human must fix, not a transient error worth retrying.
WARM_ERROR: str | None = None


@app.on_event("startup")
async def warm() -> None:
    """Load weights at startup, never on the first request.

    A cold load inside a request will blow a live demo's latency budget. A failure here leaves the
    worker running and reporting itself unready rather than killing the process: the endpoint stays
    up to answer /health, which is what tells you what went wrong.
    """
    global WARM_ERROR
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    LOGGER.info("warming generator %s", generator.name)
    try:
        await generator.warm()
    except Exception as exc:
        WARM_ERROR = f"{type(exc).__name__}: {exc}"
        LOGGER.exception("generator %s failed to warm", generator.name)
        return
    LOGGER.info("generator %s ready in %.1fs", generator.name, time.monotonic() - started)


@app.get("/health")
async def health() -> dict[str, object]:
    payload: dict[str, object] = {
        "status": "ok" if generator.ready else "unready",
        "generator": generator.name,
        "version": generator.version,
        "ready": generator.ready,
        "contract": CONTRACT_VERSION,
    }
    if WARM_ERROR is not None:
        payload["error"] = WARM_ERROR
    return payload


def _cache_key(image: bytes, prompt: str, seed: int, target_triangles: int) -> str:
    digest = hashlib.sha256()
    digest.update(image)
    digest.update(b"\0")
    digest.update(prompt.strip().encode("utf-8"))
    digest.update(b"\0")
    digest.update(str(seed).encode("ascii"))
    digest.update(b"\0")
    digest.update(str(target_triangles).encode("ascii"))
    digest.update(b"\0")
    digest.update(generator.version.encode("utf-8"))
    return digest.hexdigest()


@app.post("/v1/image-to-3d")
async def image_to_3d(
    reference: UploadFile = FileField(),
    prompt: str = Form(...),
    seed: int = Form(0),
    target_triangles: int = Form(2500),
    part_schema: str = Form("[]"),
    authorization: str | None = Header(default=None),
    x_monkeyforge_contract: str | None = Header(default=None),
) -> Response:
    if TOKEN:
        expected = f"Bearer {TOKEN}"
        if authorization != expected:
            raise HTTPException(status_code=401, detail="invalid or missing bearer token")
    if x_monkeyforge_contract != CONTRACT_VERSION:
        raise HTTPException(
            status_code=400,
            detail=f"unsupported contract version: {x_monkeyforge_contract!r}",
        )

    image = await reference.read(MAX_REFERENCE_BYTES + 1)
    if not image:
        raise HTTPException(status_code=400, detail="reference image is empty")
    if len(image) > MAX_REFERENCE_BYTES:
        raise HTTPException(status_code=413, detail="reference image exceeds 10 MiB")

    try:
        parts = json.loads(part_schema)
        if not isinstance(parts, list):
            raise ValueError("part_schema must be a JSON array")
        parts = [str(part) for part in parts]
    except (json.JSONDecodeError, ValueError) as exc:
        raise HTTPException(status_code=400, detail=f"invalid part_schema: {exc}") from exc

    if not generator.ready:
        detail = (
            f"generator failed to warm: {WARM_ERROR}"
            if WARM_ERROR
            else "generator is still warming"
        )
        raise HTTPException(status_code=503, detail=detail)

    key = _cache_key(image, prompt, seed, target_triangles)
    cached = CACHE_DIR / f"{key}.glb"
    if cached.exists():
        LOGGER.info("cache hit %s", key[:12])
        return _glb_response(cached.read_bytes(), cache_hit=True)

    started = time.monotonic()
    try:
        glb = await generator.generate(
            image=image,
            prompt=prompt,
            seed=seed,
            target_triangles=target_triangles,
            part_schema=parts,
        )
    except Exception as exc:
        LOGGER.exception("generation failed")
        raise HTTPException(status_code=500, detail=f"generation failed: {exc}") from exc

    elapsed = time.monotonic() - started
    if not glb.startswith(b"glTF"):
        raise HTTPException(status_code=500, detail="generator did not produce a binary GLB")

    cached.write_bytes(glb)
    LOGGER.info("generated %s in %.1fs (%d bytes)", key[:12], elapsed, len(glb))
    return _glb_response(glb, cache_hit=False, elapsed=elapsed)


def _glb_response(glb: bytes, cache_hit: bool, elapsed: float | None = None) -> Response:
    headers = {
        "X-Generator": generator.name,
        "X-Generator-Version": generator.version,
        "X-Cache": "hit" if cache_hit else "miss",
    }
    if elapsed is not None:
        headers["X-Generation-Seconds"] = f"{elapsed:.2f}"
    return Response(content=glb, media_type="model/gltf-binary", headers=headers)
