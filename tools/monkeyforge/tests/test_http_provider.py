"""Contract tests for the HTTP geometry provider, against a fake worker.

These run with no GPU and no network. They exist so the orchestrator side of
`docs/gpu-worker-contract.md` is pinned before a real worker is pointed at it: a failure against the
GX10 should mean the worker is wrong, not that the client was never checked.
"""

from __future__ import annotations

import json
from pathlib import Path

import httpx
import pytest

from monkeyforge.models import AccessorySpec, Socket
from monkeyforge.providers import HttpGeometryProvider

MINIMAL_GLB = b"glTF" + b"\x02\x00\x00\x00" + b"\x00" * 16


def accessory_spec() -> AccessorySpec:
    return AccessorySpec(
        id="helmet",
        slot=Socket.HEAD_TOP,
        kind="helmet",
        description="a rounded goalkeeper helmet",
        target_triangles=2500,
        part_schema=["shell", "crest"],
    )


@pytest.fixture
def sketch(tmp_path: Path) -> Path:
    path = tmp_path / "sketch.png"
    path.write_bytes(b"fake-png-bytes")
    return path


@pytest.mark.asyncio
async def test_request_matches_the_documented_contract(
    tmp_path: Path, sketch: Path, monkeypatch
) -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["headers"] = dict(request.headers)
        captured["body"] = request.content
        return httpx.Response(200, content=MINIMAL_GLB)

    _install(monkeypatch, handler)
    provider = HttpGeometryProvider(
        endpoint="https://worker.test/v1/image-to-3d", token="secret-token"
    )
    await provider.generate(accessory_spec(), sketch, seed=7, output_dir=tmp_path / "out")

    assert captured["headers"]["x-monkeyforge-contract"] == "1"
    assert captured["headers"]["authorization"] == "Bearer secret-token"

    body = captured["body"].decode("utf-8", errors="replace")
    assert "a rounded goalkeeper helmet" in body
    assert 'name="seed"' in body and "7" in body
    assert 'name="target_triangles"' in body and "2500" in body
    assert json.dumps(["shell", "crest"]) in body
    assert "fake-png-bytes" in body


@pytest.mark.asyncio
async def test_glb_is_written_and_described(tmp_path: Path, sketch: Path, monkeypatch) -> None:
    _install(monkeypatch, lambda request: httpx.Response(200, content=MINIMAL_GLB))
    provider = HttpGeometryProvider(endpoint="https://worker.test/v1/image-to-3d")

    generated = await provider.generate(
        accessory_spec(), sketch, seed=1, output_dir=tmp_path / "out"
    )

    assert generated.provider == "http"
    assert generated.media_type == "model/gltf-binary"
    assert generated.path.read_bytes() == MINIMAL_GLB


@pytest.mark.asyncio
async def test_omits_authorization_when_no_token_is_configured(
    tmp_path: Path, sketch: Path, monkeypatch
) -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["headers"] = dict(request.headers)
        return httpx.Response(200, content=MINIMAL_GLB)

    _install(monkeypatch, handler)
    provider = HttpGeometryProvider(endpoint="https://worker.test/v1/image-to-3d")
    await provider.generate(accessory_spec(), sketch, seed=1, output_dir=tmp_path / "out")

    assert "authorization" not in captured["headers"]


@pytest.mark.asyncio
async def test_non_glb_response_is_rejected(tmp_path: Path, sketch: Path, monkeypatch) -> None:
    # A worker returning an HTML error page with a 200 must not be written out as geometry.
    _install(monkeypatch, lambda request: httpx.Response(200, content=b"<html>oops</html>"))
    provider = HttpGeometryProvider(endpoint="https://worker.test/v1/image-to-3d")

    with pytest.raises(ValueError, match="did not return a binary GLB"):
        await provider.generate(accessory_spec(), sketch, seed=1, output_dir=tmp_path / "out")


@pytest.mark.asyncio
async def test_oversized_response_is_rejected(tmp_path: Path, sketch: Path, monkeypatch) -> None:
    _install(monkeypatch, lambda request: httpx.Response(200, content=b"glTF" + b"\x00" * 4096))
    provider = HttpGeometryProvider(
        endpoint="https://worker.test/v1/image-to-3d", max_response_bytes=1024
    )

    with pytest.raises(ValueError, match="exceeds configured size limit"):
        await provider.generate(accessory_spec(), sketch, seed=1, output_dir=tmp_path / "out")


@pytest.mark.asyncio
async def test_worker_error_status_propagates(tmp_path: Path, sketch: Path, monkeypatch) -> None:
    _install(
        monkeypatch,
        lambda request: httpx.Response(503, json={"detail": "generator is still warming"}),
    )
    provider = HttpGeometryProvider(endpoint="https://worker.test/v1/image-to-3d")

    with pytest.raises(httpx.HTTPStatusError):
        await provider.generate(accessory_spec(), sketch, seed=1, output_dir=tmp_path / "out")


def _install(monkeypatch, handler) -> None:
    """Route every AsyncClient the provider builds through a MockTransport."""
    transport = httpx.MockTransport(handler)
    original = httpx.AsyncClient

    def patched(*args, **kwargs):
        kwargs["transport"] = transport
        return original(*args, **kwargs)

    monkeypatch.setattr(httpx, "AsyncClient", patched)
