<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

<script setup>
import { data } from './results.data.ts'
</script>

# Latest benchmark run

Generated from the most recent benchmark run. This page is written by CI — see
[`scripts/bench-to-json.py`](https://github.com/elioseverojunior/glaucus/blob/main/scripts/bench-to-json.py)
— and is **not** the curated baseline. For the reviewed figures, with their
caveats and methodology, see [Baseline](./baseline).

<div v-if="data.degraded" class="danger custom-block">
  <p class="custom-block-title">⚠ This run is marked degraded — do not cite it</p>
  <p>{{ data.meta.note }}</p>
</div>

| | |
|---|---|
| Generated | {{ data.meta.generated_at }} |
| Commit | `{{ data.meta.commit }}` on `{{ data.meta.branch }}` |
| Toolchain | `{{ data.meta.toolchain }}` |
| Platform | {{ data.meta.platform }} |
| Runner | {{ data.meta.runner }} |
| Measurements | {{ data.count }} |

## Relative performance

Each cell is the library's time divided by glaucus's on the same fixture, so
**above 1.00x means slower than glaucus** and below means faster. Ratios lead
here because they survive runner noise: every library in a group is measured on
the same machine in the same run, so a host-wide slowdown largely cancels out of
the ratio even when it wrecks the absolute times.

<div v-for="g in data.comparisons" :key="'r-' + g.name">

### `{{ g.name }}`

<table>
  <thead>
    <tr>
      <th>Library</th>
      <th v-for="f in g.fixtures" :key="f">{{ f }}</th>
    </tr>
  </thead>
  <tbody>
    <tr v-for="lib in g.libraries" :key="lib">
      <td><code>{{ lib }}</code><span v-if="lib === 'glaucus'"> (baseline)</span></td>
      <td v-for="f in g.fixtures" :key="f">{{ g.ratios[lib][f] ?? '—' }}</td>
    </tr>
  </tbody>
</table>

</div>

## Absolute times

Medians. Treat these as indicative unless the run is known to come from a quiet,
dedicated machine — shared CI runners routinely vary by tens of percent between
runs, which is why the ratios above are the figures worth tracking.

<div v-for="g in data.comparisons" :key="'a-' + g.name">

### `{{ g.name }}`

<table>
  <thead>
    <tr>
      <th>Library</th>
      <th v-for="f in g.fixtures" :key="f">{{ f }}</th>
    </tr>
  </thead>
  <tbody>
    <tr v-for="lib in g.libraries" :key="lib">
      <td><code>{{ lib }}</code></td>
      <td v-for="f in g.fixtures" :key="f">{{ g.cells[lib][f] ?? '—' }}</td>
    </tr>
  </tbody>
</table>

</div>

## Single-implementation groups

Groups whose benchmark ids carry no library segment — glaucus-only paths, with no
competitor to compare against.

<div v-for="g in data.flat" :key="g.name">

### `{{ g.name }}`

<table>
  <thead>
    <tr><th>Case</th><th>Median</th><th>Spread</th></tr>
  </thead>
  <tbody>
    <tr v-for="r in g.rows" :key="r.label">
      <td><code>{{ r.label }}</code></td>
      <td>{{ r.time }}</td>
      <td>{{ r.spread }}</td>
    </tr>
  </tbody>
</table>

</div>
