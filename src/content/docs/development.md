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

The site uses Astro and Starlight. With Node.js 22.12 or newer in the Node 22 series and npm installed, run:

```bash
npm ci
npm run check
npm run build
```

For local editing, `npm run dev` starts the documentation server. Edit pages in `src/content/docs/`, the sidebar in `astro.config.mjs`, and the theme in `src/styles/docs.css`.

GitHub Actions builds the site with the repository's Pages origin and base path. Pushes to `main` that change documentation or site assets trigger deployment. The GitHub Wiki Home links to the published guides; the Pages site is the full documentation.

[Return to getting started →](../getting-started/)
