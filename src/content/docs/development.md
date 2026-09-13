---
title: "Development"
description: "Run the Rust checks and work on the Astro documentation site."
---

## Development checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- \
  -D clippy::pedantic -D clippy::nursery -D warnings
cargo test
cargo test --all-features --examples
cargo bench --bench ga
cargo bench --bench ga --features bench-internals -- phases
```

## Work on the docs

The site uses Astro and Starlight. With Node.js 22.12 or newer in the Node 22 series, npm, Python 3.10+, and Rust installed, run:

```bash
npm ci
npm run check
npm run build
npm run check:links
npm run test:docs
npm run check:examples
```

`check:links` scans the built HTML for missing pages, assets, and section anchors, including legacy bookmarks. It uses the build's canonical URL to respect the GitHub Pages base path. `test:docs` checks that these validators catch broken fixtures.

`check:examples` runs only the small CLI examples marked with `docs-check` comments and compares their JSON fields with the nearby `docs-result` blocks. It also compares the CLI reference's flags, aliases, and defaults against `--help`. Runtime is deliberately excluded. No native GUI is opened by these checks.

When changing CLI defaults or examples, update the reference and expected output in the same change. When adding a guide, update the sidebar and `scripts/wiki-home.py` navigation list. CI builds and checks both the site root and a repository subpath, and the Pages workflow checks links again before uploading.

For local editing, `npm run dev` starts the documentation server. Edit pages in `src/content/docs/`, the sidebar in `astro.config.mjs`, and the theme in `src/styles/docs.css`.

GitHub Actions builds the site with the repository's Pages origin and base path. Pushes to `main` that change documentation or site assets trigger deployment. The GitHub Wiki Home links to the published guides; the Pages site is the full documentation.

[Return to getting started →](../getting-started/)
