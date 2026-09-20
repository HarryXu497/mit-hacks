"""Drawing in, game-ready character out. One command, both machines.

Until now the pipeline was automated in pieces with a person in the middle: run the reader over
ssh, copy the spec back, run the generator, copy the sprites back, then invoke the local build by
hand. Every one of those steps was scriptable; none of them was scripted. This is that glue.

The split of work is not arbitrary. The three generative models are large and need CUDA, so they
live on the GX10. Everything after them -- UV projection, the height map, Blender assembly -- is
CPU work, and Blender is not installed on the box, so it runs here. This drives both.

Two latency decisions are baked in:

  * Every sprite an outfit needs is generated in **one** SDXL load (`gen_outfit.py`). Loading the
    pipeline costs 37s against 12s of actual generation, so paying it per sprite was the single
    worst thing in the pipeline.
  * Accessory meshing (TRELLIS, ~125s) runs **concurrently** with the local texture build rather
    than after it, because they share no inputs. The character is dressed while the cap is still
    being reconstructed.

Run:
    python scripts/forge_character.py --sketch input/suit_monkey.png --out-dir output/characters/x
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from monkeyforge.garments import garment_prompt  # noqa: E402
from monkeyforge.wardrobe import ACCESSORIES  # noqa: E402

#: Things worth generating a sprite for because they will become geometry. A tie is absent on
#: purpose: it lies flat and is drawn into the garment image instead.
GEOMETRY_WORDS = ("cap", "hat", "helmet", "crown", "glasses", "goggles", "visor", "mask",
                  "backpack", "cape", "headband")


def build_jobs(spec: dict, seed: int) -> list[dict]:
    """Decide every sprite this outfit needs, and exactly how to ask for each one."""
    jobs = [{"name": "garment", "prompt": garment_prompt(spec), "seed": seed}]
    if spec.get("trousers"):
        jobs.append({
            "name": "trousers",
            "prompt": ("a pair of formal dress trousers, two trouser legs only, waistband at "
                       "top, no jacket no shirt no sleeves, laid flat"),
            "seed": seed + 3,
        })
    for item in spec.get("accessories", []):
        word = str(item).strip().lower()
        if any(key in word for key in GEOMETRY_WORDS):
            jobs.append({
                "name": f"accessory_{word.replace(' ', '_')}",
                "prompt": f"one single {word}, only one {word}, three quarter view",
                "seed": seed + 7,
            })
    return jobs

#: Matches the `Host gx10` block in ~/.ssh/config.
HOST = "gx10"
REMOTE = "~/garments"
ACTIVATE = f"source ~/trellis-env/bin/activate; export HF_HUB_DISABLE_XET=1; cd {REMOTE}"


def run(command: list[str], label: str, quiet: bool = False) -> str:
    started = time.monotonic()
    result = subprocess.run(command, capture_output=True, text=True)
    elapsed = time.monotonic() - started
    if result.returncode != 0:
        print(f"[{label}] FAILED after {elapsed:.1f}s")
        print(result.stderr[-2500:] or result.stdout[-2500:])
        raise SystemExit(f"{label} failed")
    if not quiet:
        for line in result.stdout.splitlines():
            if any(key in line for key in ("wrote", "OUTFIT", "ASSEMBLE", "reach=", "stamped",
                                           "covers", "filled", "regions", "spikes")):
                print(f"  {line.strip()}")
    print(f"[{label}] {elapsed:.1f}s")
    return result.stdout


def ssh(script: str, label: str, quiet: bool = False) -> str:
    return run(["ssh", "-o", "BatchMode=yes", HOST, f"{ACTIVATE}; {script}"], label, quiet)


def scp(source: str, destination: str, label: str) -> None:
    run(["scp", "-o", "BatchMode=yes", source, destination], label, quiet=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sketch", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--name", default=None, help="run name on the box; defaults to the sketch")
    parser.add_argument("--base-texture", type=Path,
                        default=Path("output/base/skin_features.png"),
                        help="restyled body artwork; pass the stock atlas to skip restyling")
    parser.add_argument("--shell-offset", type=float, default=0.018)
    parser.add_argument("--relief-strength", type=float, default=1.0)
    parser.add_argument("--view", default="three_quarter")
    parser.add_argument("--resolution", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=11)
    parser.add_argument("--no-accessory", action="store_true",
                        help="skip TRELLIS; texture only, which is ~125s faster")
    parser.add_argument("--python", default=sys.executable)
    args = parser.parse_args()

    if not args.sketch.exists():
        raise SystemExit(f"no sketch at {args.sketch}")
    name = args.name or args.sketch.stem
    args.out_dir.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()

    print(f"== forging {name} from {args.sketch} ==")

    # 1. Read the drawing. The cheapest step and the one that decides everything after it, so it
    #    goes first and its answer is printed for the user immediately.
    ssh(f"mkdir -p {REMOTE}/runs/{name}", "prepare", quiet=True)
    scp(str(args.sketch), f"{HOST}:{REMOTE}/runs/{name}/sketch.png", "upload sketch")
    reading = ssh(
        f"python scripts/read_sketch_vlm.py runs/{name}/sketch.png --max-tokens 40 2>/dev/null",
        "read drawing", quiet=True,
    )
    try:
        spec = json.loads(reading[reading.index("{"): reading.rindex("}") + 1])
    except (ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"could not read the drawing: {error}\n{reading[:400]}") from error

    spec_path = args.out_dir / "spec.json"
    spec_path.write_text(json.dumps(spec, indent=2), encoding="utf-8")
    print(f"  drawing says: {spec.get('garment')!r}"
          f"{', text ' + repr(spec['text']) if spec.get('text') else ''}"
          f"{', trousers' if spec.get('trousers') else ''}"
          f"{', ' + ', '.join(spec['accessories']) if spec.get('accessories') else ''}")

    # 2. Generate every sprite the outfit needs, in one model load. Prompts are built here, where
    #    the garment vocabulary is importable and tested -- asking the box for the VLM's own word
    #    produced an armoured spacesuit when the drawing showed a business suit.
    jobs = build_jobs(spec, args.seed)
    jobs_path = args.out_dir / "jobs.json"
    jobs_path.write_text(json.dumps(jobs, indent=2), encoding="utf-8")
    for job in jobs:
        print(f"  {job['name']}: {job['prompt']}")
    scp(str(jobs_path), f"{HOST}:{REMOTE}/runs/{name}/jobs.json", "upload jobs")
    ssh(f"python scripts/gen_outfit.py --jobs runs/{name}/jobs.json "
        f"--out-dir runs/{name} 2>&1 | grep -E 'OUTFIT|Error'", "generate sprites")
    scp(f"{HOST}:{REMOTE}/runs/{name}/sprites.json", str(args.out_dir / "sprites.json"),
        "fetch manifest")
    sprites = json.loads((args.out_dir / "sprites.json").read_text(encoding="utf-8"))
    for key in sprites:
        scp(f"{HOST}:{REMOTE}/runs/{name}/{sprites[key]}", str(args.out_dir / sprites[key]),
            f"fetch {key}")

    # 3. Accessory meshing and the local texture build share nothing, so run them together. This
    #    is where most of the wall-clock saving is: TRELLIS is ~125s and the local build ~40s.
    accessory_keys = [key for key in sprites if key.startswith("accessory_")]
    accessory_glb: Path | None = None
    accessory_word = ""

    def mesh_accessory() -> Path | None:
        if args.no_accessory or not accessory_keys:
            return None
        key = accessory_keys[0]
        ssh(f"python scripts/image_to_glb.py --image runs/{name}/{sprites[key]} "
            f"--output runs/{name}/{key}.glb --seed 1 2>&1 | tail -2", "mesh accessory")
        local = args.out_dir / f"{key}.glb"
        scp(f"{HOST}:{REMOTE}/runs/{name}/{key}.glb", str(local), "fetch accessory")
        return local

    def build_texture() -> None:
        command = [
            args.python, "scripts/dress_monkey.py",
            "--spec", str(spec_path),
            "--garment", str(args.out_dir / sprites["garment"]),
            # The sleeves take their fabric from the garment's own image: diffusion draws a jacket
            # hanging with its sleeves angled down, and the base stands in a T-pose.
            "--sleeve", str(args.out_dir / sprites["garment"]),
            "--sleeve-crop", "0.14,0.35,0.30,0.72",
            "--out-dir", str(args.out_dir),
            "--shell-offset", str(args.shell_offset),
            "--relief-strength", str(args.relief_strength),
            "--view", args.view,
            "--resolution", str(args.resolution),
        ]
        if "trousers" in sprites:
            command += ["--trousers", str(args.out_dir / sprites["trousers"])]
        if args.base_texture.exists():
            command += ["--base-texture", str(args.base_texture)]
        run(command, "build character")

    with ThreadPoolExecutor(max_workers=2) as pool:
        meshing = pool.submit(mesh_accessory)
        pool.submit(build_texture).result()
        accessory_glb = meshing.result()

    # 4. Socket the accessory once it lands. A second assembly pass, not a re-run: the texture is
    #    already built, so this only adds the mesh.
    if accessory_glb is not None and accessory_glb.exists():
        word = accessory_keys[0].removeprefix("accessory_").replace("_", " ")
        socket = next((s for key, s in ACCESSORIES.items() if key in word), None)
        if socket is None:
            print(f"  ! no socket known for {word!r}; leaving it off")
        else:
            accessory_word = word
            from monkeyforge.models import SOCKET_NODE_NAMES
            texture = next((args.out_dir / candidate for candidate in
                            ("texture_full.png", "texture_sleeves.png", "texture_torso.png")
                            if (args.out_dir / candidate).exists()), None)
            command = [
                r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe",
                "--background", "--python", "scripts/blender_assemble_character.py", "--",
                "--base", "assets/reference/btd6_extract/Dart Monkey/dartmonkey.obj",
                "--texture", str(texture),
                "--shell", str(args.out_dir / "coverage.png"),
                "--shell-offset", str(args.shell_offset),
                "--height-map", str(args.out_dir / "height.png"),
                "--accessory", str(accessory_glb),
                "--socket", SOCKET_NODE_NAMES[socket],
                "--accessory-size", "0.80", "--sink", "0.22",
                "--glb", str(args.out_dir / "character.glb"),
                "--render", str(args.out_dir / f"render_{args.view}.png"),
                "--view", args.view, "--resolution", str(args.resolution),
            ]
            if args.base_texture.exists():
                command += ["--base-texture", str(args.base_texture)]
            run(command, "socket accessory")

    total = time.monotonic() - started
    print(f"\n== {name} forged in {total:.0f}s ==")
    print(f"   {args.out_dir / 'character.glb'}")
    print(f"   {args.out_dir / f'render_{args.view}.png'}")
    if accessory_word:
        print(f"   accessory: {accessory_word}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
