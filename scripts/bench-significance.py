#!/usr/bin/env -S uv run --script

# SPDX-FileCopyrightText: Glaucus contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///

"""Decide whether one benchmark arm beats another, from Criterion's intervals.

    cargo criterion -p glaucus-bench --bench streaming_vs_tree
    scripts/bench-significance.py --baseline tree --candidate streaming

Reads `target/criterion/**/new/estimates.json` -- the confidence intervals, not
the bencher-format point estimates, which drop them.

`significant` is the column that decides, and it is true only when the two
confidence intervals do NOT overlap. A point-estimate ratio between two noisy
measurements says nothing: on 2026-08-01 a degraded host produced 77 internally
consistent figures that were 4.6x-20.9x wrong in absolute terms. Overlapping
intervals are reported as "no", however large the ratio looks.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import sys
from pathlib import Path


def interval(estimates: dict, stat: str) -> dict[str, float] | None:
    """One statistic's point estimate and confidence interval."""
    value = estimates.get(stat)
    if not value:
        return None
    return {
        "point": value["point_estimate"],
        "low": value["confidence_interval"]["lower_bound"],
        "high": value["confidence_interval"]["upper_bound"],
    }


def load(root: Path) -> dict[str, dict[str, dict[str, float]]]:
    """Every measurement under `root`, keyed by its Criterion id.

    Both `slope` and `mean` are kept. Criterion's headline `time:` line is the
    SLOPE -- a regression over iteration counts, which is far less sensitive to
    the occasional long iteration a busy host produces. It is absent for slow
    benchmarks, where one iteration per sample leaves no line to fit, so `mean`
    has to serve there.
    """
    out: dict[str, dict[str, dict[str, float]]] = {}
    for path in root.glob("**/new/estimates.json"):
        # .../criterion/<group>/<id>/<param>/new/estimates.json
        parts = path.relative_to(root).parts[:-2]
        if not parts:
            continue
        estimates = json.loads(path.read_text())
        stats = {
            name: found
            for name in ("slope", "mean")
            if (found := interval(estimates, name)) is not None
        }
        if stats:
            out["/".join(parts)] = stats
    return out


def swap_arm(key: str, candidate: str, baseline: str) -> str | None:
    """The key naming the same case on the other arm, or `None`.

    Substituted one path segment at a time, never across the whole string.
    Criterion flattens `partial/streaming` to `partial_streaming`, and a blind
    replace would also rewrite the GROUP name -- turning `streaming_vs_tree`
    into `tree_vs_tree` and pairing nothing.
    """
    parts = key.split("/")
    for i, part in enumerate(parts):
        if part == candidate or part.endswith(f"_{candidate}"):
            parts[i] = part[: len(part) - len(candidate)] + baseline
            return "/".join(parts)
    return None


def strip_arm(key: str, arm: str) -> str:
    """`key` with the arm name removed, keeping what distinguishes the case.

    `generic_streaming` becomes `generic`, not nothing: dropping the whole
    segment would collapse every shape onto one label and silently overwrite
    rows that measured different things.
    """
    parts = []
    for part in key.split("/"):
        if part == arm:
            continue
        if part.endswith(f"_{arm}"):
            parts.append(part[: -len(arm) - 1])
            continue
        parts.append(part)
    return "/".join(parts)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", type=Path, default=Path("target/criterion"))
    ap.add_argument("--baseline", default="tree", help="substring naming the baseline arm")
    ap.add_argument("--candidate", default="streaming", help="substring naming the candidate arm")
    ap.add_argument("--json", type=Path, help="also write the rows here")
    ap.add_argument(
        "--host",
        type=Path,
        help="preflight snapshot taken BEFORE the run; quoted rather than re-sampled",
    )
    args = ap.parse_args()

    if not args.root.is_dir():
        print(f"no criterion output at {args.root}; run the benchmark first", file=sys.stderr)
        return 2

    measurements = load(args.root)
    if not measurements:
        print(f"no estimates.json under {args.root}", file=sys.stderr)
        return 2

    rows = []
    for key, candidate in sorted(measurements.items()):
        baseline_key = swap_arm(key, args.candidate, args.baseline)
        if baseline_key is None:
            continue
        baseline = measurements.get(baseline_key)
        if baseline is None:
            continue

        # Slope when BOTH arms have it, mean otherwise. Never mixed within a
        # pair: comparing one arm's slope against the other's mean would compare
        # two different quantities and call the difference a result.
        stat = "slope" if "slope" in candidate and "slope" in baseline else "mean"
        if stat not in candidate or stat not in baseline:
            continue
        c, b = candidate[stat], baseline[stat]

        # Non-overlapping intervals: the candidate's WORST case still beats the
        # baseline's BEST case, or vice versa. Anything else is a tie.
        faster = c["high"] < b["low"]
        slower = c["low"] > b["high"]
        rows.append(
            {
                "case": strip_arm(key, args.candidate),
                "stat": stat,
                "candidate_ns": c["point"],
                "baseline_ns": b["point"],
                "speedup": b["point"] / c["point"],
                "significant": "faster" if faster else "slower" if slower else "no",
            }
        )

    if not rows:
        print("no paired measurements found", file=sys.stderr)
        return 2

    width = max(len(r["case"]) for r in rows)
    header = f"{'case'.ljust(width)}  {'candidate':>12}  {'baseline':>12}  {'speedup':>8}  {'stat':>5}  significant"
    print(header)
    print("-" * len(header))
    for r in rows:
        print(
            f"{r['case'].ljust(width)}  "
            f"{r['candidate_ns'] / 1000:>10.2f}us  "
            f"{r['baseline_ns'] / 1000:>10.2f}us  "
            f"{r['speedup']:>7.2f}x  "
            f"{r['stat']:>5}  "
            f"{r['significant']}"
        )

    # Recorded with the rows, not alongside them. `bench-to-json.py` marks a run
    # `degraded` for the same reason: a contaminated run that publishes itself
    # unqualified is how 2026-08-01 happened. Noise widens intervals, so a
    # "faster" verdict from a busy host is conservative -- but a "no" from one
    # is inconclusive, and the reader has to be able to tell which they hold.
    #
    # Quoted from the snapshot the gate took BEFORE the run. Re-sampling here
    # would describe a machine that has since gone quiet, and label the run
    # clean on the strength of conditions it never ran under.
    if args.host and args.host.exists():
        snapshot = json.loads(args.host.read_text())
        quality, busy = snapshot["quality"], snapshot["busy"]
        cores = snapshot.get("cores", os.cpu_count() or 1)
        arch = snapshot.get("arch", "unknown")
        host_platform = snapshot.get("platform", "unknown")
    else:
        cores = os.cpu_count() or 1
        busy = os.getloadavg()[0] / cores
        quality = "unverified"
        arch = platform.machine()
        host_platform = platform.platform()
    print()
    print(f"host: {arch}, {busy:.1%} busy over {cores} cores -> {quality}")

    if args.json:
        payload = {
            "quality": quality,
            "busy": round(busy, 4),
            "cores": cores,
            "arch": arch,
            "platform": host_platform,
            "rows": rows,
        }
        args.json.write_text(json.dumps(payload, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
