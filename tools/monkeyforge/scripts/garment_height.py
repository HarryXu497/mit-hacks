"""Derive a height map from flat garment art, so cloth details read as raised instead of printed.

No machine learning and no depth estimation model. Cel-shaded garment art is *already* segmented:
a tie, a lapel and a shirt are flat areas of distinct colour separated by dark outlines. That
structure is the depth information, and three classical operations recover it:

  1. **Quantise and label.** Reducing to a small palette and taking connected components turns the
     drawing into named regions -- this patch is the tie, that patch is the lapel.

  2. **Distance transform inside each region.** The distance from a pixel to its region's edge,
     normalised per region, rounds every patch into a pad that is highest at its middle and falls
     to nothing at its border. This is what makes a tie look like a strip of cloth lying on a
     shirt rather than a stripe painted on it.

  3. **Rank regions by area.** In flat art the small shapes are the ones drawn on top -- buttons,
     a pocket square, a tie over a shirt front. Giving smaller regions a higher base recovers the
     layering the artist implied, without anything needing to know what a tie is.

Dark outlines are then pressed *in*, because an artist's linework marks a seam or a fold, which is
a groove rather than a ridge.

The result drives a bump node -- shading only, no extra geometry, and it costs nothing at runtime.
Feed it to a displacement instead if real silhouette is wanted.

Run:
    python scripts/garment_height.py --texture <projected.png> --coverage <mask.png> \
        --output <height.png>
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter


def label_regions(mask: np.ndarray, min_area: int) -> list[tuple[np.ndarray, int]]:
    """Connected components of a boolean mask, as (mask, area), largest first.

    Written out rather than imported: scipy's compiled extensions are blocked by this machine's
    application-control policy, and the garment covers a small enough part of the atlas that a
    flood fill over it is immediate.
    """
    labels = np.zeros(mask.shape, dtype=np.int32)
    found: list[tuple[np.ndarray, int]] = []
    current = 0
    for seed in zip(*np.nonzero(mask), strict=True):
        if labels[seed]:
            continue
        current += 1
        stack = [seed]
        labels[seed] = current
        area = 0
        while stack:
            y, x = stack.pop()
            area += 1
            for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
                if 0 <= ny < mask.shape[0] and 0 <= nx < mask.shape[1]:
                    if mask[ny, nx] and not labels[ny, nx]:
                        labels[ny, nx] = current
                        stack.append((ny, nx))
        if area >= min_area:
            found.append((labels == current, area))
    found.sort(key=lambda item: item[1], reverse=True)
    return found


def distance_to_edge(region: np.ndarray) -> np.ndarray:
    """Distance from each pixel to the region's boundary, by repeated erosion.

    Erosion counts how many times a pixel survives shrinking the shape, which is its distance to
    the edge. Working inside the region's bounding box keeps this to a handful of small filter
    passes even for a large patch.
    """
    ys, xs = np.nonzero(region)
    y0, y1 = int(ys.min()), int(ys.max())
    x0, x1 = int(xs.min()), int(xs.max())
    # One pixel of padding so the shape's own border erodes rather than the crop's edge.
    patch = np.zeros((y1 - y0 + 3, x1 - x0 + 3), dtype=bool)
    patch[1:-1, 1:-1] = region[y0 : y1 + 1, x0 : x1 + 1]

    distance = np.zeros(patch.shape, dtype=np.float32)
    current = patch
    for _ in range(max(patch.shape)):
        if not current.any():
            break
        distance += current
        eroded = Image.fromarray((current * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(3))
        current = np.asarray(eroded) > 127

    out = np.zeros(region.shape, dtype=np.float32)
    out[y0 : y1 + 1, x0 : x1 + 1] = distance[1:-1, 1:-1]
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True,
                        help="the projected texture, in UV space, so the height lines up with it")
    parser.add_argument("--coverage", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--colours", type=int, default=10,
                        help="palette size for segmentation; too many splits flat cloth into "
                             "shading bands, too few merges the tie into the shirt")
    parser.add_argument("--layer-weight", type=float, default=0.55,
                        help="how much a region's height comes from being a small detail on top")
    parser.add_argument("--round-weight", type=float, default=0.45,
                        help="how much comes from the distance-to-edge rounding")
    parser.add_argument("--groove", type=float, default=0.55,
                        help="how deeply to press in the artist's dark outlines")
    parser.add_argument("--blur", type=float, default=1.6,
                        help="smooths the quantisation's stair-steps; cloth has no hard terraces")
    parser.add_argument("--min-region", type=int, default=24)
    args = parser.parse_args()

    texture = Image.open(args.texture).convert("RGB")
    covered = np.asarray(
        Image.open(args.coverage).convert("L").resize(texture.size, Image.NEAREST)
    ) > 127
    if not covered.any():
        raise SystemExit("coverage mask is empty")

    rgb = np.asarray(texture, dtype=np.float32)
    luminance = rgb @ np.array([0.299, 0.587, 0.114], dtype=np.float32)

    # Quantise only what the garment covers, so the base body's colours cannot claim palette slots
    # and blur the garment's own segmentation.
    masked = Image.fromarray(
        np.where(covered[..., None], np.asarray(texture), 0).astype(np.uint8)
    )
    palette = masked.quantize(colors=args.colours, method=Image.MEDIANCUT)
    indices = np.asarray(palette)

    height = np.zeros(texture.size[::-1], dtype=np.float32)
    regions: list[tuple[np.ndarray, int]] = []
    for value in np.unique(indices[covered]):
        regions.extend(label_regions((indices == value) & covered, args.min_region))
    if not regions:
        raise SystemExit("no regions found; try --colours or a lower --min-region")

    areas = np.array([area for _, area in regions], dtype=np.float32)
    largest = areas.max()
    print(f"  {len(regions)} regions, areas {int(areas.min())}-{int(largest)} px")

    for region, area in regions:
        # Small regions sit on top of large ones. The cube root keeps a tie noticeably proud of a
        # shirt front without making a button tower over everything.
        layer = 1.0 - (area / largest) ** (1.0 / 3.0)
        # Normalised per region so a narrow tie rounds as fully as a broad lapel does.
        distance = distance_to_edge(region)
        peak = distance.max()
        rounding = distance / peak if peak > 0 else distance
        height[region] = (args.layer_weight * layer
                          + args.round_weight * rounding[region])

    # The artist's linework is a seam or a fold: press it in rather than letting it read as a rim.
    dark = covered & (luminance < np.percentile(luminance[covered], 18))
    height[dark] -= args.groove
    print(f"  pressed {int(dark.sum())} outline px in by {args.groove}")

    height = np.clip(height, 0.0, 1.0)
    image = Image.fromarray((height * 255).astype(np.uint8))
    if args.blur > 0:
        image = image.filter(ImageFilter.GaussianBlur(args.blur))

    # Outside the garment there is no cloth; flat keeps the bump node from inventing relief there.
    flattened = np.asarray(image).copy()
    flattened[~covered] = 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(flattened).save(args.output)
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
