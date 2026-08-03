// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

// Assert every internal `nav`/`sidebar` link in config.mts resolves to a page
// the build actually emitted.
//
// This exists because VitePress's `ignoreDeadLinks: false` does NOT cover the
// theme config. It dead-link-checks links written in markdown CONTENT; the nav
// and sidebar are plain data it never validates. The consequence was not
// hypothetical: every one of the seven configured links 404'd on the published
// site while `vitepress build` reported success with zero warnings, and the
// only two real pages were absent from the sidebar entirely.
//
// config.mts already carried a comment promising "every entry below resolves to
// a page that exists" -- and every entry below it was dead. A comment cannot
// enforce an invariant; this can.
//
// Run AFTER `vitepress build`, against the emitted `dist`, rather than guessing
// markdown paths: `cleanUrls`, directory indexes and `base` all change how a
// link maps to a file, and dist is the only place that mapping is already
// resolved.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const config = resolve(here, "config.mts");
const dist = resolve(here, "dist");

if (!existsSync(dist)) {
  console.error(
    `check-nav-links: ${dist} not found -- run \`bun run build\` first.`,
  );
  process.exit(2);
}

// Only double-quoted absolute paths. Repository links are written as template
// literals (`${REPO}/releases`), so this pattern skips external targets without
// needing to special-case them.
const source = readFileSync(config, "utf8");
const links = [...source.matchAll(/link:\s*"(\/[^"]*)"/g)].map((m) => m[1]);

if (links.length === 0) {
  console.error(
    "check-nav-links: no internal links found -- has config.mts moved?",
  );
  process.exit(2);
}

// `/` -> index.html, `/a/` -> a/index.html, `/a` -> a.html (cleanUrls) or
// a/index.html, whichever the build chose.
const candidates = (link) => {
  const path = link.replace(/^\//, "");
  if (path === "") return ["index.html"];
  if (path.endsWith("/")) return [`${path}index.html`];
  return [`${path}.html`, `${path}/index.html`];
};

const dead = [...new Set(links)]
  .sort()
  .filter((link) => !candidates(link).some((c) => existsSync(join(dist, c))));

if (dead.length > 0) {
  console.error(
    `check-nav-links: ${dead.length} configured link(s) resolve to no emitted page:`,
  );
  for (const link of dead) console.error(`  ${link}`);
  console.error(
    "\nEither add the page under docs/, or remove the entry from nav/sidebar in config.mts.",
  );
  process.exit(1);
}

console.log(`check-nav-links: ${new Set(links).size} internal link(s) OK`);
