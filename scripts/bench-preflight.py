#!/usr/bin/env -S uv run --script

# SPDX-FileCopyrightText: Glaucus contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///

"""Refuse to benchmark on a host too busy for the numbers to mean anything.

    scripts/bench-preflight.py            # gate: exits non-zero when contaminated
    scripts/bench-preflight.py --report   # always exits 0, prints the verdict

This exists because of 2026-08-01, when a degraded host produced 77 internally
consistent measurements that were 4.6x-20.9x wrong in absolute terms. Tight
confidence intervals did not save that run: every arm was slow together, so
nothing inside the data looked wrong. The only defence is refusing to measure in
the first place, which is what this does.

Noise widens confidence intervals, so a contaminated run is more likely to report
"not significant" than a false "faster". A verdict of NOT-faster from a busy host
is therefore inconclusive rather than evidence -- which matters when the decision
on the table is whether to delete an engine.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import sys
from pathlib import Path

# Above this share of the machine, another process is competing for the cores the
# benchmark needs and the run is not worth starting.
MAX_LOAD_PER_CORE = 0.30


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--report", action="store_true", help="print the verdict but always exit 0")
    ap.add_argument("--max-load", type=float, default=MAX_LOAD_PER_CORE)
    ap.add_argument(
        "--snapshot",
        type=Path,
        help="write the verdict here, for the report to quote afterwards",
    )
    args = ap.parse_args()

    cores = os.cpu_count() or 1
    load1, load5, _ = os.getloadavg()
    busy = load1 / cores

    print(f"cores:      {cores}")
    print(f"load 1m/5m: {load1:.2f} / {load5:.2f}")
    print(f"busy:       {busy:.1%} (threshold {args.max_load:.0%})")

    quality = "ok" if busy <= args.max_load else "degraded"
    if args.snapshot:
        # Written BEFORE the run. Sampling the load afterwards would describe a
        # machine that has since gone quiet and label a contaminated run clean --
        # which is the failure this whole gate exists to prevent.
        args.snapshot.write_text(
            json.dumps(
                {
                    "quality": quality,
                    "busy": round(busy, 4),
                    "cores": cores,
                    # Recorded because the ANSWER depends on it: the tree path's
                    # cost is dominated by building a node graph, and how cheap
                    # that is differs enough between architectures to change the
                    # size of the win by more than 2x.
                    "arch": platform.machine(),
                    "platform": platform.platform(),
                }
            )
            + "\n"
        )

    if busy <= args.max_load:
        print("verdict:    QUIET - measurements are decision-grade")
        return 0

    print("verdict:    CONTAMINATED - measurements are NOT decision-grade")
    print()
    print("Another process is using the cores this benchmark needs. Absolute")
    print("figures will be wrong and confidence intervals will be wide, so a")
    print("'not significant' result proves nothing. Close other work and re-run.")
    return 0 if args.report else 1


if __name__ == "__main__":
    sys.exit(main())
