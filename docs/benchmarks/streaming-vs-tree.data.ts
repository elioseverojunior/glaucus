// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineLoader } from "vitepress";

// One file per host. Globbed rather than named, so recording another machine is
// dropping a file in — and so a reader sees every host the decision rests on
// rather than whichever one happened to be written last.
const DIR = fileURLToPath(new URL("./runs", import.meta.url));

interface Row {
  case: string;
  // Criterion's slope estimate where it has one, mean otherwise. Never mixed
  // within a pair.
  stat: "slope" | "mean";
  candidate_ns: number;
  baseline_ns: number;
  speedup: number;
  // "faster" and "slower" mean the confidence intervals do not overlap. "no"
  // means they do, whatever the speedup column says.
  significant: "faster" | "slower" | "no";
}

interface Raw {
  quality: "ok" | "degraded" | "unverified";
  busy: number;
  cores: number;
  arch: string;
  platform: string;
  rows: Row[];
}

interface Run {
  arch: string;
  platform: string;
  cores: number;
  quality: string;
  busy: string;
  rows: Array<Row & { streaming_us: string; tree_us: string; ratio: string }>;
  /** True when medium AND large are significantly faster — the #58 rule. */
  verdict: boolean;
}

export interface Data {
  runs: Run[];
  /** True only when EVERY recorded host meets the rule. */
  verdict: boolean;
}

declare const data: Data;
export { data };

function format(raw: Raw): Run {
  const decisive = (name: string) =>
    raw.rows.some((r) => r.case.includes(name) && r.significant === "faster") &&
    raw.rows
      .filter((r) => r.case.includes(name))
      .every((r) => r.significant === "faster");

  return {
    arch: raw.arch,
    platform: raw.platform,
    cores: raw.cores,
    quality: raw.quality,
    busy: `${(raw.busy * 100).toFixed(1)}%`,
    rows: raw.rows.map((r) => ({
      ...r,
      streaming_us: (r.candidate_ns / 1000).toFixed(2),
      tree_us: (r.baseline_ns / 1000).toFixed(2),
      ratio: `${r.speedup.toFixed(2)}x`,
    })),
    verdict: decisive("medium") && decisive("large"),
  };
}

export default defineLoader({
  watch: [`${DIR}/*.json`],
  load(): Data {
    let runs: Run[] = [];
    try {
      runs = readdirSync(DIR)
        .filter((f) => f.endsWith(".json"))
        .map((f) => format(JSON.parse(readFileSync(`${DIR}/${f}`, "utf-8")) as Raw));
    } catch {
      // No run recorded yet. An empty table is honest; a fabricated one is not.
      return { runs: [], verdict: false };
    }

    return {
      runs,
      // Every host, not any host: a rule met on one architecture and missed on
      // another has not been met.
      verdict: runs.length > 0 && runs.every((r) => r.verdict),
    };
  },
});
