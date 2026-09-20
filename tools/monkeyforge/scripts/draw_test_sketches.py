"""Draw a small evaluation set of rough doodles, one file per motif.

These exist so the zero-shot classifier can be measured against *something* before
real users draw anything. They are deliberately built from different shape code than
`powers.badge._GLYPHS`, and deliberately wobbled, so the test is not the placeholder
renderer grading its own homework.

They are still synthetic. Treat the resulting number as "the CLIP path works and is
not grossly miscalibrated", not as human-sketch accuracy. Replace them with real
drawings as soon as anyone draws some.

    .\\.venv\\Scripts\\python.exe scripts\\draw_test_sketches.py --output-dir output\\sketches
"""

from __future__ import annotations

import argparse
import json
import math
import random
from collections.abc import Callable
from pathlib import Path

from PIL import Image, ImageDraw

from monkeyforge.powers.registry import PowerId

SIZE = 512
INK = (32, 32, 40)
PAPER = (255, 255, 255)
Point = tuple[float, float]


class Pencil:
    """A wobbly pen. Every stroke is jittered so nothing comes out machine-clean."""

    def __init__(self, draw: ImageDraw.ImageDraw, rng: random.Random, wobble: float = 5.0) -> None:
        self.draw = draw
        self.rng = rng
        self.wobble = wobble

    def _jitter(self, point: Point) -> Point:
        return (
            point[0] + self.rng.uniform(-self.wobble, self.wobble),
            point[1] + self.rng.uniform(-self.wobble, self.wobble),
        )

    def stroke(self, points: list[Point], width: int = 7, close: bool = False) -> None:
        path = [self._jitter(p) for p in points]
        if close:
            path.append(path[0])
        # Two passes at slightly different offsets read as pencil rather than vector.
        for _ in range(2):
            self.draw.line(
                [self._jitter(p) for p in path], fill=INK, width=width, joint="curve"
            )

    def circle(self, centre: Point, radius: float, width: int = 7, steps: int = 40) -> None:
        self.stroke(
            [
                (
                    centre[0] + math.cos(2 * math.pi * i / steps) * radius,
                    centre[1] + math.sin(2 * math.pi * i / steps) * radius,
                )
                for i in range(steps)
            ],
            width=width,
            close=True,
        )


def _polar(centre: Point, radius: float, degrees: float) -> Point:
    angle = math.radians(degrees)
    return centre[0] + math.cos(angle) * radius, centre[1] + math.sin(angle) * radius


C: Point = (SIZE / 2, SIZE / 2)


def explosion(pen: Pencil) -> None:
    points = [
        _polar(C, 170 if i % 2 == 0 else 70, i * 30 + pen.rng.uniform(-8, 8)) for i in range(12)
    ]
    pen.stroke(points, close=True)


def raygun(pen: Pencil) -> None:
    pen.stroke([(120, 300), (260, 300), (260, 250), (150, 250), (150, 300)], close=True)
    pen.stroke([(150, 300), (140, 370), (185, 370), (195, 300)], close=True)
    for offset in (-45, 0, 45):
        pen.stroke([(265, 275 + offset * 0.3), (430, 275 + offset)], width=6)


def blast_rings(pen: Pencil) -> None:
    pen.circle(C, 40)
    for radius in (95, 150, 205):
        pen.stroke([_polar(C, radius, d) for d in range(-60, 61, 6)], width=6)


def snowflake(pen: Pencil) -> None:
    for spoke in range(6):
        angle = spoke * 60.0
        pen.stroke([C, _polar(C, 180, angle)])
        root = _polar(C, 105, angle)
        for side in (-38, 38):
            pen.stroke([root, _polar(root, 62, angle + side)], width=6)


def icicles(pen: Pencil) -> None:
    pen.stroke([(90, 150), (422, 150)])
    for index in range(6):
        x = 110 + index * 60
        pen.stroke([(x, 155), (x + 26, 155), (x + 13, 250 + (index % 3) * 55)], close=True, width=6)


def ice_cube(pen: Pencil) -> None:
    pen.stroke([(150, 200), (300, 140), (400, 200), (250, 265)], close=True)
    pen.stroke([(150, 200), (150, 340), (250, 405), (250, 265)], close=True)
    pen.stroke([(250, 265), (250, 405), (400, 340), (400, 200)], close=True)


def bolt(pen: Pencil) -> None:
    pen.stroke(
        [(300, 90), (170, 290), (250, 290), (205, 430), (345, 235), (262, 235)],
        close=True,
    )


def spring(pen: Pencil) -> None:
    points: list[Point] = []
    for step in range(140):
        t = step / 139
        points.append((175 + math.sin(t * math.pi * 9) * 95, 100 + t * 320))
    pen.stroke(points, width=7)
    pen.stroke([(120, 90), (330, 90)], width=6)
    pen.stroke([(120, 430), (330, 430)], width=6)


def rocket(pen: Pencil) -> None:
    pen.stroke([(256, 90), (200, 210), (200, 330), (312, 330), (312, 210)], close=True)
    pen.stroke([(200, 270), (140, 350), (200, 330)], close=True, width=6)
    pen.stroke([(312, 270), (372, 350), (312, 330)], close=True, width=6)
    for x in (225, 256, 287):
        pen.stroke([(x, 335), (x + pen.rng.uniform(-18, 18), 440)], width=6)


def hourglass(pen: Pencil) -> None:
    pen.stroke([(140, 110), (372, 110)])
    pen.stroke([(140, 402), (372, 402)])
    pen.stroke([(160, 110), (256, 256), (160, 402)], width=6)
    pen.stroke([(352, 110), (256, 256), (352, 402)], width=6)
    pen.stroke([(256, 256), (256, 340)], width=5)


def snail(pen: Pencil) -> None:
    shell: list[Point] = []
    for step in range(120):
        t = step / 119
        shell.append(_polar((280, 240), 20 + t * 115, t * 900))
    pen.stroke(shell, width=6)
    pen.stroke([(190, 330), (120, 330), (100, 300), (95, 250)], width=6)
    pen.stroke([(95, 250), (88, 215)], width=5)
    pen.stroke([(120, 255), (112, 218)], width=5)
    pen.stroke([(150, 355), (330, 355)], width=6)


def clock(pen: Pencil) -> None:
    pen.circle(C, 165)
    for hour in range(12):
        pen.stroke([_polar(C, 140, hour * 30), _polar(C, 160, hour * 30)], width=5)
    pen.stroke([C, _polar(C, 110, -90)], width=7)
    pen.stroke([C, _polar(C, 75, 30)], width=7)


MOTIFS: tuple[tuple[str, PowerId, Callable[[Pencil], None]], ...] = (
    ("explosion", PowerId.BEAM_BLAST, explosion),
    ("raygun", PowerId.BEAM_BLAST, raygun),
    ("blast_rings", PowerId.BEAM_BLAST, blast_rings),
    ("snowflake", PowerId.FREEZE_RAY, snowflake),
    ("icicles", PowerId.FREEZE_RAY, icicles),
    ("ice_cube", PowerId.FREEZE_RAY, ice_cube),
    ("bolt", PowerId.BOOST, bolt),
    ("spring", PowerId.BOOST, spring),
    ("rocket", PowerId.BOOST, rocket),
    ("hourglass", PowerId.SLOW, hourglass),
    ("snail", PowerId.SLOW, snail),
    ("clock", PowerId.SLOW, clock),
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=Path("output/sketches"))
    parser.add_argument("--seed", type=int, default=7)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = []
    for index, (name, expected, render) in enumerate(MOTIFS):
        image = Image.new("RGB", (SIZE, SIZE), PAPER)
        render(Pencil(ImageDraw.Draw(image), random.Random(args.seed + index)))
        path = args.output_dir / f"{name}.png"
        image.save(path)
        manifest.append({"sketch": path.name, "motif": name, "expected": str(expected)})
        print(f"{name:12s} -> {path}  (expects {expected})")

    index_path = args.output_dir / "expected.json"
    index_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"{'manifest':12s} -> {index_path}")


if __name__ == "__main__":
    main()
