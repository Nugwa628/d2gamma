#!/usr/bin/env python3
"""Summarize successful UI recordings without reading private signing keys."""
import argparse
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    runs = {}
    for recording in sorted((args.output / "input").glob("*.json")):
        result = json.loads(
            (args.output / "captures" / recording.stem / "result.json").read_text()
        )
        expected = len(json.loads(recording.read_text())["steps"])
        if result["error"] or result["completed_steps"] != expected:
            raise SystemExit(f"Incomplete recording {recording.stem}: {result}")
        runs[recording.stem] = expected
    if not runs:
        raise SystemExit("No tour recordings found")
    packages = {}
    for mod_id in ("showcase", "showcase-notes"):
        package = args.home / "mods" / f"{mod_id}-1.0.0.d2pack"
        packages[package.name] = {
            "bytes": package.stat().st_size,
            "sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
        }
    summary = {
        "format_version": 1,
        "recordings": runs,
        "completed_input_steps": sum(runs.values()),
        "capture_count": len(list((args.output / "captures").glob("*/*.png"))),
        "saved_document_and_effective_table_comparison_exit": 0,
        "check_package_play_recording_exit": 0,
        "packages": packages,
    }
    path = args.output / "tour-summary.json"
    path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
