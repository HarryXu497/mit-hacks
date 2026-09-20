"""Restyle the base monkey's own artwork so the character reads as ours, not as a lift.

The mesh is a legitimate reference to build the pipeline against, but its texture is instantly
recognisable: a very specific orange fur over a pale tan face. Changing the palette breaks that
recognition while keeping everything that makes the base good -- the UV layout, the hand-painted
shading, the eye shapes the mesh's alpha cutout depends on.

Three deliberate constraints:

* **Only the character's own tile is touched.** The source is a 4x4 atlas of several characters;
  restyling all of it would alter art this character never samples.
* **Hue rotation, not replacement.** The artwork's light and shade live in the value channel, so
  rotating hue and scaling saturation keeps every painted highlight and crease intact. A flat
  recolour would throw that away and look like a paint bucket.
* **Neutral pixels are left alone.** Eye whites, pupils and the dark outlines carry no hue, so
  rotating them does nothing but risk tinting the eyes. They are held out by a saturation floor.

Run:
    python scripts/restyle_base.py --texture <atlas.png> --output <restyled.png> \
        --hue 150 --saturation 0.82
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image

#: The dart monkey occupies one cell of the 4x4 atlas: u 0.00-0.25, v 0.25-0.50. In image pixels,
#: with v measured from the bottom, that is the left edge and the second row up.
DEFAULT_TILE = (0.0, 0.25, 0.25, 0.50)


def parse_hex(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def rgb_to_hsv(rgb: np.ndarray) -> np.ndarray:
    """Vectorised RGB->HSV on a float array in [0, 1]."""
    maximum = rgb.max(axis=-1)
    minimum = rgb.min(axis=-1)
    delta = maximum - minimum

    hue = np.zeros_like(maximum)
    red, green, blue = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    safe = delta > 1e-6
    is_red = safe & (maximum == red)
    is_green = safe & (maximum == green)
    is_blue = safe & (maximum == blue)
    with np.errstate(invalid="ignore", divide="ignore"):
        hue[is_red] = ((green - blue)[is_red] / delta[is_red]) % 6
        hue[is_green] = ((blue - red)[is_green] / delta[is_green]) + 2
        hue[is_blue] = ((red - green)[is_blue] / delta[is_blue]) + 4
    hue = hue / 6.0

    saturation = np.zeros_like(maximum)
    nonzero = maximum > 1e-6
    saturation[nonzero] = delta[nonzero] / maximum[nonzero]
    return np.stack([hue, saturation, maximum], axis=-1)


def hsv_to_rgb(hsv: np.ndarray) -> np.ndarray:
    hue, saturation, value = hsv[..., 0], hsv[..., 1], hsv[..., 2]
    index = np.floor(hue * 6.0).astype(np.int32) % 6
    fraction = hue * 6.0 - np.floor(hue * 6.0)
    p = value * (1.0 - saturation)
    q = value * (1.0 - fraction * saturation)
    t = value * (1.0 - (1.0 - fraction) * saturation)

    red = np.select([index == 0, index == 1, index == 2, index == 3, index == 4, index == 5],
                    [value, q, p, p, t, value])
    green = np.select([index == 0, index == 1, index == 2, index == 3, index == 4, index == 5],
                      [t, value, value, q, p, p])
    blue = np.select([index == 0, index == 1, index == 2, index == 3, index == 4, index == 5],
                     [p, p, t, value, value, q])
    return np.stack([red, green, blue], axis=-1)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--hue", type=float, default=150.0,
                        help="degrees to rotate the palette; the source fur sits near 25 (orange)")
    parser.add_argument("--saturation", type=float, default=0.82,
                        help="multiplier; below 1 reads as a softer, less cartoon-saturated coat")
    parser.add_argument("--value", type=float, default=1.0,
                        help="brightness multiplier")
    parser.add_argument("--saturation-floor", type=float, default=0.12,
                        help="pixels flatter than this keep their colour: eye whites, pupils and "
                             "the dark outlines, which must not pick up a tint")
    parser.add_argument("--tile", type=float, nargs=4, default=DEFAULT_TILE,
                        metavar=("U0", "U1", "V0", "V1"),
                        help="the character's cell of the atlas, in UV")
    parser.add_argument("--whole-atlas", action="store_true",
                        help="restyle everything instead of just this character's tile")
    args = parser.parse_args()

    image = Image.open(args.texture).convert("RGB")
    width, height = image.size
    data = np.asarray(image, dtype=np.float32) / 255.0

    if args.whole_atlas:
        region = (slice(0, height), slice(0, width))
    else:
        u0, u1, v0, v1 = args.tile
        # v runs bottom-up in UV space and top-down in image rows.
        region = (slice(int((1.0 - v1) * height), int((1.0 - v0) * height)),
                  slice(int(u0 * width), int(u1 * width)))
        print(f"  tile rows {region[0].start}:{region[0].stop} "
              f"cols {region[1].start}:{region[1].stop}")

    patch = data[region]
    hsv = rgb_to_hsv(patch)

    # Hold out neutrals so eyes and outlines survive untouched.
    coloured = hsv[..., 1] > args.saturation_floor
    print(f"  restyling {int(coloured.sum())} of {coloured.size} px "
          f"({coloured.mean():.0%}); {int((~coloured).sum())} neutral px held out")

    hsv[..., 0] = np.where(coloured, (hsv[..., 0] + args.hue / 360.0) % 1.0, hsv[..., 0])
    hsv[..., 1] = np.where(coloured, np.clip(hsv[..., 1] * args.saturation, 0, 1), hsv[..., 1])
    hsv[..., 2] = np.clip(hsv[..., 2] * args.value, 0, 1)

    data[region] = hsv_to_rgb(hsv)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray((np.clip(data, 0, 1) * 255).astype(np.uint8)).save(args.output)
    print(f"wrote {args.output}  (hue {args.hue:+.0f} deg, saturation x{args.saturation})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
