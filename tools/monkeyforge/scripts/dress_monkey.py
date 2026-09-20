"""Dress the base monkey from a spec read off a drawing.

This is the join between the two halves of the pipeline. Reading the drawing happens elsewhere --
`read_sketch_vlm.py` on a GPU box for the open-ended fields, `read_garment.py` for the closed one --
and produces a spec. This turns that spec into a character, and it is the only place that decides
how a spec becomes build arguments.

The rules it encodes, each of which came from a measurement rather than a guess:

* Sleeve length comes from the garment's *name*, not from looking at the arms. Both models read
  arms unreliably (CLIP split 36/34 on a t-shirt; Qwen answered "long" for a t-shirt and a suit)
  and both name garments correctly. See `monkeyforge.garments`.
* Lettering is stamped, never generated. Diffusion cannot spell.
* Conforming garments become texture; things with a silhouette become socketed geometry. That
  split lives in `monkeyforge.wardrobe` and is the reason a cap is a mesh and a shirt is not.

Run:
    python scripts/dress_monkey.py --spec spec.json --garment input/garments/jersey_clean.png \
        --out-dir output/characters/mit
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from monkeyforge.garments import resolve_sleeves  # noqa: E402
from monkeyforge.models import SOCKET_NODE_NAMES  # noqa: E402
from monkeyforge.wardrobe import ACCESSORIES, CONFORMING  # noqa: E402

BASE = Path("assets/reference/btd6_extract/Dart Monkey/dartmonkey.obj")
ATLAS = Path("assets/reference/btd6_extract/Dart Monkey/1580ad4c89b8819bf46a68a680fde598.png")
LAYOUT = Path("output/uv_debug/layout_pos.json")
ISLANDS = Path("output/uv_debug/islands/islands.json")

#: Trunk front, trunk back, and the arms. The arms are included because the shoulders live in that
#: island: without them a shirt stops at the armpit and reads as a bib.
TORSO_ISLANDS = ("5", "9", "1")
#: The arms alone, for when sleeves need their own image.
ARM_ISLANDS = ("1",)
#: The legs, for trousers. Island 4 only: island 3 is the feet, which trousers stop above, and
#: island 2 sits at y=6.7 -- well behind the body -- so it is the tail, not a leg.
LEG_ISLANDS = ("4",)

BLENDER = Path(r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe")


def run(command: list[str], label: str) -> None:
    print(f"\n[{label}] {' '.join(str(part) for part in command[:3])} ...")
    result = subprocess.run(command, capture_output=True, text=True)
    for line in result.stdout.splitlines():
        if any(key in line for key in ("wrote", "reach=", "stamped", "ASSEMBLE", "covers")):
            print(f"  {line.strip()}")
    if result.returncode != 0:
        print(result.stderr[-2000:])
        raise SystemExit(f"{label} failed with {result.returncode}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, required=True,
                        help="JSON from read_sketch_vlm.py")
    parser.add_argument("--garment", type=Path, required=True,
                        help="generated garment sprite to wear")
    parser.add_argument("--trousers", type=Path, default=None,
                        help="generated sprite for the legs, when the spec asks for trousers")
    parser.add_argument("--sleeve", type=Path, default=None,
                        help="separate sprite for the sleeves. Diffusion draws a jacket hanging "
                             "with its sleeves angled down, while the base stands in a T-pose, so "
                             "mapping the arms straight off the garment image runs off the cloth "
                             "and leaves bare patches mid-arm. Giving the sleeves their own image "
                             "-- a swatch of the same fabric -- covers the arm continuously")
    parser.add_argument("--sleeve-crop", default=None,
                        help="sub-rectangle of --sleeve to use, as x0,y0,x1,y1 fractions")
    parser.add_argument("--accessory", type=Path, action="append", default=[],
                        help="GLB for an accessory the spec named, in the order it named them")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--source-crop", default=None)
    parser.add_argument("--text-fraction", type=float, default=0.30)
    parser.add_argument("--text-height", type=float, default=0.46)
    parser.add_argument("--accessory-size", type=float, default=0.80)
    parser.add_argument("--sink", type=float, default=0.22)
    parser.add_argument("--base-texture", type=Path, default=None,
                        help="restyled artwork for the body itself, from restyle_base.py")
    parser.add_argument("--flat", action="store_true",
                        help="paint the garment into the base texture instead of raising it as "
                             "its own layer; cheaper, but it reads as printed on the skin")
    parser.add_argument("--shell-offset", type=float, default=0.014,
                        help="how far the garment stands off the body, as a fraction of height")
    parser.add_argument("--fill-gaps", type=int, default=40, metavar="RADIUS",
                        help="how far to grow cloth into holes the garment's silhouette leaves "
                             "on the body; 0 disables")
    parser.add_argument("--spike-count", type=int, default=7)
    parser.add_argument("--spike-length", type=float, default=0.085,
                        help="spike length as a fraction of body height")
    parser.add_argument("--no-relief", action="store_true",
                        help="skip the height map; the garment stays smooth")
    parser.add_argument("--relief-strength", type=float, default=0.6)
    parser.add_argument("--view", default="front")
    parser.add_argument("--resolution", type=int, default=1000)
    parser.add_argument("--python", default=sys.executable)
    args = parser.parse_args()

    spec = json.loads(args.spec.read_text(encoding="utf-8"))
    garment_name = str(spec.get("garment", ""))
    sleeve, reach, source = resolve_sleeves(garment_name)
    text = str(spec.get("text", "")).strip()
    wants_trousers = bool(spec.get("trousers")) and args.trousers is not None

    print(f"spec: garment={garment_name!r} text={text!r} "
          f"accessories={spec.get('accessories', [])}")
    print(f"  sleeves -> {sleeve} (reach {reach:.2f}) from {source}")

    args.out_dir.mkdir(parents=True, exist_ok=True)
    torso_texture = args.out_dir / "texture_torso.png"
    coverage = args.out_dir / "coverage.png"
    # Each pass adds to the mask, so start from nothing rather than from the last run's outfit.
    coverage.unlink(missing_ok=True)

    # When the sleeves get their own image the body pass covers the trunk only; otherwise it
    # covers the arms too, which is what keeps a t-shirt continuous over the shoulders.
    body_islands = TORSO_ISLANDS if args.sleeve is None else ("5", "9")

    project = [
        args.python, "scripts/project_planar.py",
        "--texture", str(ATLAS),
        "--garment", str(args.garment),
        "--layout", str(LAYOUT),
        "--islands", str(ISLANDS),
        "--select", *body_islands,
        "--output", str(torso_texture),
        "--coverage", str(coverage),
        # Every face picked for the body is meant to be covered, so close any wedge the garment's
        # own silhouette would otherwise leave as bare fur.
        "--fill-gaps", str(args.fill_gaps),
        "--text-fraction", str(args.text_fraction),
        "--text-height", str(args.text_height),
    ]
    if args.sleeve is None:
        # Reach measures how far down the *arm* the garment goes, so it only belongs on a pass
        # that includes the arm island. Applied to a trunk-only pass it trims the garment's sides
        # instead, because the trunk's own span is the thing being cut.
        project += ["--reach", f"{reach:.4f}"]
    if text:
        project += ["--text", text]
    if args.source_crop:
        project += ["--source-crop", args.source_crop]
    run(project, "garment")

    final_texture = torso_texture

    if args.sleeve is not None and reach > 0:
        sleeve_texture = args.out_dir / "texture_sleeves.png"
        sleeve_pass = [
            args.python, "scripts/project_planar.py",
            "--texture", str(final_texture),
            "--garment", str(args.sleeve),
            "--layout", str(LAYOUT),
            "--islands", str(ISLANDS),
            "--select", *ARM_ISLANDS,
            "--reach", f"{reach:.4f}",
            "--output", str(sleeve_texture),
            "--coverage", str(coverage),
        ]
        if args.sleeve_crop:
            sleeve_pass += ["--source-crop", args.sleeve_crop]
        run(sleeve_pass, "sleeves")
        final_texture = sleeve_texture
    if wants_trousers:
        # Trousers are the same operation on a different set of islands, which is the point of
        # projecting by position: nothing about the code knows "legs" specifically.
        previous = final_texture
        final_texture = args.out_dir / "texture_full.png"
        run([
            args.python, "scripts/project_planar.py",
            "--texture", str(previous),
            "--garment", str(args.trousers),
            "--layout", str(LAYOUT),
            "--islands", str(ISLANDS),
            "--select", *LEG_ISLANDS,
            "--output", str(final_texture),
            "--coverage", str(coverage),
        ], "trousers")

    assemble = [
        str(BLENDER), "--background", "--python", "scripts/blender_assemble_character.py", "--",
        "--base", str(BASE),
        "--texture", str(final_texture),
        "--glb", str(args.out_dir / "character.glb"),
        "--render", str(args.out_dir / f"render_{args.view}.png"),
        "--view", args.view,
        "--resolution", str(args.resolution),
        "--accessory-size", str(args.accessory_size),
        "--sink", str(args.sink),
    ]
    for region in spec.get("spikes", []):
        # The drawing only says *where*; the shape comes from the base's own proportions.
        assemble += ["--spike", str(region)]
    if spec.get("spikes"):
        assemble += ["--spike-count", str(args.spike_count),
                     "--spike-length", str(args.spike_length)]
        print(f"  spikes on {', '.join(spec['spikes'])}")

    if args.base_texture:
        assemble += ["--base-texture", str(args.base_texture)]
    if not args.flat and coverage.exists():
        assemble += ["--shell", str(coverage), "--shell-offset", str(args.shell_offset)]
        if not args.no_relief:
            # Recover depth from the garment art's own region structure, so a tie sits proud of a
            # shirt and the artist's linework reads as seams rather than as drawn-on stripes.
            height_map = args.out_dir / "height.png"
            run([
                args.python, "scripts/garment_height.py",
                "--texture", str(final_texture),
                "--coverage", str(coverage),
                "--output", str(height_map),
            ], "relief")
            assemble += ["--height-map", str(height_map),
                         "--height-strength", str(args.relief_strength)]
    # Each named accessory is matched to its socket by the wardrobe table, so a cap goes on the
    # head and sunglasses on the face without this script knowing anything about either.
    named = [str(item).lower() for item in spec.get("accessories", [])]
    for index, glb in enumerate(args.accessory):
        word = named[index] if index < len(named) else ""
        socket = next((s for key, s in ACCESSORIES.items() if key in word), None)
        if socket is None:
            # A VLM lists a tie under "things that stick out", but a tie lies flat and is painted
            # with the garment. Saying so beats a bare "skipping", which reads like a failure.
            if any(key in word for key in CONFORMING):
                print(f"  {word!r} lies flat - it is painted with the garment, not socketed")
            else:
                print(f"  ! no socket known for {word!r}; skipping {glb}")
            continue
        node = SOCKET_NODE_NAMES[socket]
        print(f"  accessory {word!r} -> {node}")
        assemble += ["--accessory", str(glb), "--socket", node]
    run(assemble, "assemble")

    print(f"\nwrote {args.out_dir / f'render_{args.view}.png'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
