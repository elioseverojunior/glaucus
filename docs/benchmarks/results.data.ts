// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineLoader } from "vitepress";

// Build-time loader. `watch` makes `vitepress dev` hot-reload the page when CI
// drops a new benchmarks.json in, and pins the dependency for the production
// build so a stale page cannot be served from cache.
const DATA = fileURLToPath(
  new URL("../.data/benchmarks.json", import.meta.url),
);

const FIXTURES = ["small", "medium", "large"] as const;

interface Measurement {
  id: string;
  group: string;
  library: string | null;
  fixture: string | null;
  ns_per_iter: number;
  deviation_ns: number;
}

interface Raw {
  generated_at: string;
  commit: string;
  branch: string;
  toolchain: string;
  platform: string;
  runner: string;
  quality: "ok" | "degraded";
  note: string;
  measurements: Measurement[];
}

/** A group where every id is `group/library/fixture` — renders as a matrix. */
interface ComparisonGroup {
  name: string;
  libraries: string[];
  fixtures: string[];
  /** `cells[library][fixture]` — pre-formatted time, or null when not measured. */
  cells: Record<string, Record<string, string | null>>;
  /**
   * glaucus-relative ratios. >1 means the competitor is SLOWER than glaucus.
   * Ratios are the headline because they survive runner noise: every library in
   * a group is measured on the same machine in the same run, so a host-wide
   * slowdown cancels out of the ratio while wrecking the absolute times.
   */
  ratios: Record<string, Record<string, string | null>>;
}

/** A group whose ids carry no library segment — renders as a flat list. */
interface FlatGroup {
  name: string;
  rows: { label: string; time: string; spread: string }[];
}

export interface BenchData {
  meta: Omit<Raw, "measurements">;
  degraded: boolean;
  count: number;
  comparisons: ComparisonGroup[];
  flat: FlatGroup[];
}

function fmt(ns: number): string {
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(2)} µs`;
  return `${ns.toFixed(0)} ns`;
}

declare const data: BenchData;
export { data };

export default defineLoader({
  watch: [DATA],
  load(): BenchData {
    const raw = JSON.parse(readFileSync(DATA, "utf-8")) as Raw;
    const { measurements, ...rest } = raw;
    // Fallbacks resolved here, not in the template: a `||` inside a markdown
    // table cell is split on by the table parser before Vue ever sees it, and
    // the expression fails to compile.
    const meta = { ...rest, runner: rest.runner || "—", note: rest.note || "" };

    const byGroup = new Map<string, Measurement[]>();
    for (const m of measurements) {
      const bucket = byGroup.get(m.group);
      if (bucket) bucket.push(m);
      else byGroup.set(m.group, [m]);
    }

    const comparisons: ComparisonGroup[] = [];
    const flat: FlatGroup[] = [];

    for (const [name, rows] of [...byGroup].sort(([a], [b]) =>
      a.localeCompare(b),
    )) {
      if (rows.every((r) => r.library !== null)) {
        const libraries = [...new Set(rows.map((r) => r.library as string))]
          // glaucus first; it is the subject of every comparison.
          .sort((a, b) =>
            a === "glaucus" ? -1 : b === "glaucus" ? 1 : a.localeCompare(b),
          );
        const fixtures = FIXTURES.filter((f) =>
          rows.some((r) => r.fixture === f),
        );

        const cells: ComparisonGroup["cells"] = {};
        const ratios: ComparisonGroup["ratios"] = {};
        for (const lib of libraries) {
          cells[lib] = {};
          ratios[lib] = {};
          for (const fx of fixtures) {
            const hit = rows.find((r) => r.library === lib && r.fixture === fx);
            cells[lib][fx] = hit ? fmt(hit.ns_per_iter) : null;

            const base = rows.find(
              (r) => r.library === "glaucus" && r.fixture === fx,
            );
            ratios[lib][fx] =
              hit && base && base.ns_per_iter > 0
                ? `${(hit.ns_per_iter / base.ns_per_iter).toFixed(2)}x`
                : null;
          }
        }
        comparisons.push({
          name,
          libraries,
          fixtures: [...fixtures],
          cells,
          ratios,
        });
      } else {
        flat.push({
          name,
          rows: rows.map((r) => ({
            label: r.fixture ?? r.id,
            time: fmt(r.ns_per_iter),
            spread:
              r.ns_per_iter > 0
                ? `±${((r.deviation_ns / r.ns_per_iter) * 100).toFixed(1)}%`
                : "—",
          })),
        });
      }
    }

    return {
      meta,
      degraded: raw.quality === "degraded",
      count: measurements.length,
      comparisons,
      flat,
    };
  },
});
