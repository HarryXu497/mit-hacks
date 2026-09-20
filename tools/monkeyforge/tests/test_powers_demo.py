"""HTTP contract tests for the demo app, on the placeholder generator."""

from __future__ import annotations

import io

import pytest
from fastapi.testclient import TestClient
from PIL import Image, ImageDraw

from monkeyforge.powers.badge import BadgeStyle
from monkeyforge.powers.beautify import PlaceholderGenerator
from monkeyforge.powers.classify import PowerClassifier
from monkeyforge.powers.demo import create_app
from monkeyforge.powers.pipeline import IconPipeline
from tests.test_powers_classify import FakeEmbedder


@pytest.fixture
def client():
    pipeline = IconPipeline(
        classifier=PowerClassifier(embedder=FakeEmbedder()),
        generator=PlaceholderGenerator(),
        badge_style=BadgeStyle(size=128),
    )
    return TestClient(create_app(pipeline, "placeholder"))


def sketch_bytes(motif: str) -> bytes:
    image = Image.new("RGB", (128, 128), (255, 255, 255))
    ImageDraw.Draw(image).ellipse([20, 20, 108, 108], outline=(0, 0, 0), width=4)
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    # FakeEmbedder reads the file's bytes as text; PNG readers stop at IEND.
    return buffer.getvalue() + motif.encode("utf-8")


def test_index_serves_the_page(client):
    response = client.get("/")
    assert response.status_code == 200
    assert "Draw a superpower" in response.text
    assert "v1/power-icon" in response.text, "the page must post to the real endpoint"


def test_health_reports_the_generator(client):
    assert client.get("/health").json() == {"status": "ok", "generator": "placeholder"}


def test_posting_a_sketch_returns_a_decision_and_an_icon(client):
    response = client.post(
        "/v1/power-icon",
        files={"sketch": ("sketch.png", sketch_bytes("a snowflake"), "image/png")},
        data={"description": ""},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["power"] == "freeze_ray"
    assert body["display_name"] == "Freeze Ray"
    assert body["icon"].startswith("data:image/png;base64,")
    assert set(body["scores"]) == {"beam_blast", "freeze_ray", "boost", "slow"}
    assert body["motif"] in ("a snowflake",)


def test_description_only_is_accepted(client):
    response = client.post("/v1/power-icon", data={"description": "an hourglass"})
    assert response.status_code == 200
    assert response.json()["power"] == "slow"


def test_runner_up_name_is_resolved(client):
    body = client.post("/v1/power-icon", data={"description": "a snail"}).json()
    assert body["runner_up"] != body["power"]
    assert body["runner_up_name"] and body["runner_up_name"] != body["display_name"]


def test_empty_request_is_rejected(client):
    response = client.post("/v1/power-icon", data={"description": "   "})
    assert response.status_code == 400


def test_a_non_image_upload_is_rejected_rather_than_crashing(client):
    response = client.post(
        "/v1/power-icon",
        files={"sketch": ("sketch.png", b"this is not a png", "image/png")},
        data={"description": ""},
    )
    assert response.status_code == 400
    assert "not an image" in response.json()["detail"]


def test_an_oversized_sketch_is_rejected(client):
    response = client.post(
        "/v1/power-icon",
        files={"sketch": ("big.png", b"\x00" * (9 * 1024 * 1024), "image/png")},
        data={"description": ""},
    )
    assert response.status_code == 413
