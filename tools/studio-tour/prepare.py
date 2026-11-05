#!/usr/bin/env python3
"""Prepare egui input recordings; never create or edit a mod project."""
import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--game", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    replacements = {
        "@HOME@": str(args.home.resolve()),
        "@NATIVE@": str(args.native.resolve()),
        "@GAME@": str(args.game.resolve()),
    }
    args.output.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).parent / "recordings"
    count = 0
    for path in sorted(source.glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        for step in document["steps"]:
            if isinstance(step.get("value"), str):
                for key, value in replacements.items():
                    step["value"] = step["value"].replace(key, value)
        count += len(document["steps"])
        (args.output / path.name).write_text(
            json.dumps(document, indent=2) + "\n", encoding="utf-8"
        )
    print(f"Prepared {count} ordinary egui input steps in {args.output}")


if __name__ == "__main__":
    main()
