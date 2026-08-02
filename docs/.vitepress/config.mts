// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

import { defineConfig } from "vitepress";

import { mermaidMarkdown, mermaidVite } from "./mermaid";

const REPO = "https://github.com/elioseverojunior/glaucus";

// GitHub project pages serve under `/<repo>/`, so the base has to match or every
// asset 404s. A custom domain serves from the root instead, hence the override:
// `DOCS_BASE=/ bun run build` produces a build for root-domain hosting.
//
// The trailing slash is required, not cosmetic. VitePress asserts that `base`
// both starts and ends with `/`, and every `head` entry below interpolates it
// directly — without it, `${base}favicon.svg` resolves to the single path
// segment `/rust-toolchainfavicon.svg`.
const base = process.env.DOCS_BASE ?? "/glaucus/";

// Pinned rather than left on Vite's 5173 default, which any other checkout on
// this machine also claims. `strictPort` makes a collision fail loudly instead
// of silently landing on 5174 and printing a URL nobody reads.
//
// DEV ONLY. `vitepress preview` does not run Vite's preview server -- it serves
// the built directory with Polka -- so a `vite.preview` block here is read by
// nothing and the command lands on its own 4173 default. Verified: with that
// block present, `bun run preview` still printed 4173. The preview port is
// therefore passed as a CLI flag from package.json, which honours DOCS_PORT the
// same way this does.
const port = Number(process.env.DOCS_PORT ?? 5273);

export default defineConfig({
  base,
  title: "glaucus",
  description:
    "A GitHub Action that installs a Rust toolchain from your glaucus.toml and caches cargo in layers. A superset of dtolnay/glaucus that also replaces Swatinem/rust-cache.",
  lang: "en-GB",
  cleanUrls: true,
  lastUpdated: true,

  // Dead links FAIL the build. Keep it that way: the nav and sidebar below are
  // hand-maintained against the pages that actually exist, and this check is
  // what catches an entry added ahead of its page. VitePress only checks links
  // in markdown content, NOT in `themeConfig.nav`/`sidebar` — a bad entry there
  // renders a 404 at runtime and builds clean, so those still need care.
  //
  // A page that needs to reach a repo file outside this directory uses an
  // absolute REPO url. Relative `../README.md` reaches outside the srcDir and
  // is what this check exists to catch.
  ignoreDeadLinks: false,

  // ```mermaid fences become collapsible diagrams; see .vitepress/mermaid.ts for
  // why this is wired onto mermaid directly rather than through
  // vitepress-plugin-mermaid, and theme/Mermaid.vue for the component itself.
  // Without this, every fence renders as a plain highlighted code block.
  markdown: { config: mermaidMarkdown },

  vite: {
    // Spread first: `mermaidVite` carries only `build`, `optimizeDeps` and
    // `resolve`, so the port settings below cannot collide with it. Written this
    // way round so a future key added there does not silently outrank the ports.
    ...mermaidVite,
    server: { port, strictPort: true },
  },

  head: [
    // `base`-prefixed by hand: entries in `head` are emitted verbatim, so a
    // bare "/favicon.svg" 404s on project pages served under /glaucus/.
    [
      "link",
      { rel: "icon", type: "image/svg+xml", href: `${base}favicon.svg` },
    ],
    // `alternate icon`, listed AFTER the SVG: a browser that understands
    // image/svg+xml takes the first match and ignores this, while one that does
    // not falls back here. Reversing the order would serve the 32x32 raster to
    // everyone.
    //
    // It does not silence the `GET /favicon.ico 404` in the dev console. That is
    // the browser probing the ORIGIN root, which ignores both `base` and these
    // tags -- under /glaucus/ nothing can answer it.
    [
      "link",
      {
        rel: "alternate icon",
        type: "image/x-icon",
        href: `${base}favicon.ico`,
      },
    ],
    ["meta", { name: "theme-color", content: "#1f5572" }],
    ["meta", { property: "og:type", content: "website" }],
    [
      "meta",
      {
        property: "og:title",
        content: "glaucus -- install Rust and cache cargo in one action",
      },
    ],
  ],

  themeConfig: {
    // Every entry below resolves to a page that exists in docs/. Add entries as
    // pages land, not before -- VitePress does not dead-link-check this block,
    // so an entry written ahead of its page builds clean and 404s in the browser.
    nav: [
      { text: "Architecture", link: "/ARCHITECTURE" },
      { text: "Comparison", link: "/COMPARISON" },
      { text: "Runbooks", link: "/RUNBOOKS" },
      {
        text: "Repository",
        items: [
          { text: "README", link: `${REPO}#readme` },
          { text: "Releases", link: `${REPO}/releases` },
          { text: "MIT licence", link: `${REPO}/blob/main/LICENSE-MIT` },
          {
            text: "Apache-2.0 licence",
            link: `${REPO}/blob/main/LICENSE-APACHE`,
          },
        ],
      },
    ],

    sidebar: [
      {
        text: "Reference",
        items: [
          { text: "Architecture", link: "/ARCHITECTURE" },
          { text: "Comparison", link: "/COMPARISON" },
          { text: "Runbooks", link: "/RUNBOOKS" },
        ],
      },
      {
        text: "Design records",
        items: [
          {
            text: "Layered cargo cache",
            link: "/design/2026-07-31-layered-cargo-cache",
          },
          {
            text: "Layered cargo cache — Phase B",
            link: "/design/2026-07-31-layered-cargo-cache-phase-b",
          },
        ],
      },
      {
        text: "Implementation plans",
        items: [
          {
            text: "Phase A",
            link: "/plans/2026-07-31-layered-cargo-cache-phase-a",
          },
          {
            text: "Phase B",
            link: "/plans/2026-07-31-layered-cargo-cache-phase-b",
          },
        ],
      },
    ],

    socialLinks: [{ icon: "github", link: REPO }],

    // Bundled at build time from the page content, so search needs no external
    // service and the site stays a set of static files.
    search: { provider: "local" },

    editLink: {
      pattern: `${REPO}/edit/main/docs/:path`,
      text: "Edit this page on GitHub",
    },

    footer: {
      // Two files, not one: the repository deliberately has no root `LICENSE`.
      // GitHub's `licensee` detector picks a single file and prefers a root
      // `LICENSE` over `LICENSE-*`, so an unmatchable combined file resolves the
      // repository to NOASSERTION instead of the dual licence.
      message: `Code released under <a href="${REPO}/blob/main/LICENSE-MIT">MIT</a> OR <a href="${REPO}/blob/main/LICENSE-APACHE">Apache-2.0</a>. Documentation under CC-BY-3.0+.`,
      copyright: "Copyright (c) RUST-TOOLCHAIN contributors",
    },
  },
});
