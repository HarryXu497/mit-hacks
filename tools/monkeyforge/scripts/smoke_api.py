"""Submit one real local API job and print the completed record."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from fastapi.testclient import TestClient

from monkeyforge.api import app


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=991)
    parser.add_argument(
        "--description",
        default="fast goalkeeper with a rounded star training helmet",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    sketch_path = Path(
        "assets/reference/btd6_prototype/Dart Monkey/DartMonkeyDiffuse.png"
    ).resolve()
    with TestClient(app) as client, sketch_path.open("rb") as sketch:
        response = client.post(
            "/v1/characters",
            data={
                "description": args.description,
                "seed": str(args.seed),
            },
            files={"sketch": (sketch_path.name, sketch, "image/png")},
        )
        response.raise_for_status()
        submitted = response.json()
        job = client.get(submitted["status_url"])
        job.raise_for_status()
        print(json.dumps(job.json(), indent=2))
        if job.json()["state"] != "complete":
            raise SystemExit(1)


if __name__ == "__main__":
    main()
