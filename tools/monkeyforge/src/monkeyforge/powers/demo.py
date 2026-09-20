"""A small local page for drawing a power and seeing the icon come back.

Deliberately thin: it owns HTTP and nothing else. All the behaviour lives in
`IconPipeline`, so what the page shows is exactly what the library does -- a demo that
re-implements the logic would stop being evidence that the library works.

Serve it from wherever the generator lives. With `--generator sdxl` that means the GPU
box, because the model is there; the laptop then just browses to it.
"""

from __future__ import annotations

import base64
import io
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory

from fastapi import FastAPI, Form, HTTPException, UploadFile
from fastapi.responses import HTMLResponse, JSONResponse
from PIL import Image, UnidentifiedImageError

from monkeyforge.powers.pipeline import IconPipeline

PAGE = Path(__file__).with_name("demo.html")
MAX_SKETCH_BYTES = 8 * 1024 * 1024


@dataclass
class DemoState:
    pipeline: IconPipeline
    generator_name: str


def _data_url(image: Image.Image) -> str:
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return "data:image/png;base64," + base64.b64encode(buffer.getvalue()).decode("ascii")


def create_app(pipeline: IconPipeline, generator_name: str) -> FastAPI:
    app = FastAPI(title="MonkeyForge power icons", docs_url="/docs")
    state = DemoState(pipeline=pipeline, generator_name=generator_name)

    @app.get("/", response_class=HTMLResponse)
    def index() -> str:
        return PAGE.read_text(encoding="utf-8")

    @app.get("/health")
    def health() -> dict[str, str]:
        return {"status": "ok", "generator": state.generator_name}

    @app.post("/v1/power-icon")
    async def power_icon(
        description: str = Form(default=""),
        sketch: UploadFile | None = None,
    ) -> JSONResponse:
        payload = await sketch.read() if sketch is not None else b""
        if len(payload) > MAX_SKETCH_BYTES:
            raise HTTPException(status_code=413, detail="sketch too large")
        if not payload and not description.strip():
            raise HTTPException(status_code=400, detail="draw something or describe it")

        with TemporaryDirectory() as directory:
            path = None
            if payload:
                path = Path(directory) / "sketch.png"
                path.write_bytes(payload)
                try:
                    with Image.open(path) as probe:
                        probe.verify()
                except (UnidentifiedImageError, OSError) as exc:
                    raise HTTPException(status_code=400, detail="sketch is not an image") from exc

            result = state.pipeline.run(sketch_path=path, description=description)

        guess = result.guess
        power = result.power
        return JSONResponse(
            {
                "power": str(guess.power),
                "display_name": power.display_name,
                "effect": power.effect,
                "cooldown_seconds": power.cooldown_seconds,
                "accent": power.accent,
                "confidence": guess.confidence,
                "runner_up": str(guess.runner_up),
                "runner_up_name": guess.resolved.display_name
                if guess.runner_up == guess.power
                else next(
                    p.display_name for p in pipeline.classifier.powers if p.id == guess.runner_up
                ),
                "unsure": guess.unsure,
                "message": guess.message(),
                "motif": guess.motif,
                "scores": {str(k): v for k, v in guess.scores.items()},
                "generator": result.generator,
                "icon": _data_url(result.icon),
            }
        )

    return app
