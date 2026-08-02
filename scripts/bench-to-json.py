#!/usr/bin/env -S uv run --script

# SPDX-FileCopyrightText: Glaucus contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///

"""Convert Criterion's bencher-format output into the JSON the docs site reads.

    cargo bench -p glaucus-bench -- --output-format bencher > bench-output.txt
    scripts/bench-to-json.py bench-output.txt -o docs/.data/benchmarks.json

Deliberately dumb: it emits measurements and environment, nothing derived. Every
ratio, ranking and verdict is computed in docs/benchmarks/results.data.ts, so the
numbers have exactly one representation and the presentation can change without
re-running an 85-minute benchmark.

`quality` is the field that matters. Benchmarks on shared CI runners are noisy,
and a run can be internally consistent (tight confidence intervals) while being
wholly wrong in absolute terms -- that is precisely what happened on 2026-08-01,
where every figure landed 4.6x-20.9x slow on a degraded host yet looked healthy.
The docs page keys its warning banner off this field, so mark a run `degraded`
whenever the environment is suspect rather than letting plausible-looking numbers
publish themselves unqualified.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

# `test <id> ... bench: <n> ns/iter (+/- <d>)`; thousands separators are grouped
# by Criterion, so strip them before int().
BENCHER = re.compile(
    r"^test\s+(?P<id>\S+)\s+\.\.\.\s+bench:\s+(?P<ns>[\d,]+)\s+ns/iter\s+"
    r"\(\+/-\s+(?P<dev>[\d,]+)\)"
)


def _int(text: str) -> int:
    return int(text.replace(",", ""))


def _git(*args: str) -> str:
    try:
        return subprocess.run(
            ["git", *args], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def parse(text: str) -> list[dict[str, object]]:
    """Split each benchmark id into group/library/fixture where it has that shape.

    Ids are `group/library/fixture` (serde_deserialize/glaucus/small),
    `group/fixture` (roundtrip_node/small) or bare. Anything that does not split
    into three keeps `library`/`fixture` as None rather than being guessed at;
    the loader renders those in a flat table instead of a comparison matrix.
    """
    rows: list[dict[str, object]] = []
    for line in text.splitlines():
        m = BENCHER.match(line.strip())
        if not m:
            continue
        parts = m.group("id").split("/")
        group = parts[0]
        library = parts[1] if len(parts) == 3 else None
        fixture = parts[-1] if len(parts) >= 2 else None
        rows.append(
            {
                "id": m.group("id"),
                "group": group,
                "library": library,
                "fixture": fixture,
                "ns_per_iter": _int(m.group("ns")),
                "deviation_ns": _int(m.group("dev")),
            }
        )
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input", type=Path, help="bencher-format output, or - for stdin")
    ap.add_argument("-o", "--output", type=Path, required=True)
    ap.add_argument(
        "--quality",
        choices=("ok", "degraded"),
        default="ok",
        help="mark the run suspect; the docs page renders a warning banner",
    )
    ap.add_argument("--note", default="", help="shown beside the quality banner")
    ap.add_argument("--runner", default="", help="e.g. github-hosted ubuntu-latest")
    args = ap.parse_args()

    text = sys.stdin.read() if str(args.input) == "-" else args.input.read_text()
    measurements = parse(text)
    if not measurements:
        print("bench-to-json: no bencher lines found in input", file=sys.stderr)
        return 1

    payload = {
        "generated_at": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "commit": _git("rev-parse", "--short", "HEAD"),
        "branch": _git("rev-parse", "--abbrev-ref", "HEAD"),
        "toolchain": subprocess.run(
            ["rustc", "--version"], capture_output=True, text=True, check=False
        ).stdout.strip(),
        "platform": f"{platform.system()} {platform.release()} {platform.machine()}",
        "runner": args.runner,
        "quality": args.quality,
        "note": args.note,
        "measurements": measurements,
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n")
    print(
        f"bench-to-json: wrote {len(measurements)} measurements to {args.output}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
