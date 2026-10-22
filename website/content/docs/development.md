---
title: "Development"
description: "Run the Rust checks, build the web GUI, and work on the Astro documentation site."
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

## Build the web GUI

The browser version is the `n_queens_gui` binary compiled to WebAssembly with [Trunk](https://trunkrs.dev/). Install the target and Trunk once:

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Then `trunk serve` builds `web/index.html` and serves it at <http://127.0.0.1:8080/>, rebuilding on changes. Add `--release` for realistic solver speed. Without threads, the GUI steps the solver for a slice of each frame (`SteppedWorker` in `src/gui/worker.rs`) instead of running it on a background thread. To lint that code path, run clippy for the web target too:

```bash
cargo clippy --lib --bins --features gui --target wasm32-unknown-unknown -- \
  -D clippy::pedantic -D clippy::nursery -D warnings
```

## Work on the docs

The site uses Astro and Starlight. With Node.js 22.12 or newer in the Node 22 series, npm, Python 3.10+, and Rust installed, run:

```bash
npm ci
npm run check
npm run build:gui
npm run build
npm run check:links
npm run test:docs
npm run check:examples
```

`build:gui` builds the web GUI into `public/demo/`, which the site build copies to `/demo/`. It needs the Trunk setup above, and `check:links` fails on the links to the demo without it. `check:links` scans the built HTML for missing pages, assets, and section anchors, including legacy bookmarks. It uses the build's canonical URL to respect the GitHub Pages base path. `test:docs` checks that these validators catch broken fixtures.

`check:examples` runs only the small CLI examples marked with `docs-check` comments and compares their JSON fields with the nearby `docs-result` blocks. It also compares the CLI reference's flags, aliases, and defaults against `--help`. Runtime is deliberately excluded. No native GUI is opened by these checks.

When changing CLI defaults or examples, update the reference and expected output in the same change. When adding a guide, update the sidebar and `scripts/wiki-home.py` navigation list. CI builds and checks both the site root and a repository subpath, and the Pages workflow checks links again before uploading.

For local editing, `npm run dev` starts the documentation server. The site lives in `website/`, kept separate from the Rust crate in `src/`: edit pages in `website/content/docs/`, the sidebar in `astro.config.mjs`, and the theme in `website/styles/docs.css`.

GitHub Actions builds the site with the repository's Pages origin and base path. Pushes to `main` that change documentation, site assets, or the Rust sources behind the web GUI trigger deployment. The GitHub Wiki Home links to the published guides; the Pages site is the full documentation.

## Update the GUI screenshot

The README, homepage, and GUI guide share `website/assets/gui-overview.jpg`. The current image was captured on macOS from revision `fea5200` using **Recommended defaults · 18×18**, seed **42**, which solved at epoch **39**.

1. Build and launch the current GUI with `cargo run --release --locked --features gui --bin n_queens_gui`.
2. Select the recommended preset, set seed 42, and click **Run solver**.
3. Resize the window and, if needed, reduce the interface scale with Ctrl/⌘ + **−** so the board and both charts are fully visible. Capture only the application window.
4. Replace the shared image and update the captions and this capture record if the preset, seed, or outcome changes. Keep the filename extension consistent with the image format.
5. Run the documentation build and link checks above.

The screenshot records a real run. Its elapsed time is illustrative, not a benchmark.

## GitHub Wiki sync

Enable **Wikis** under the repository's **Settings → General → Features**. The `.github/workflows/wiki-sync.yml` workflow regenerates Wiki `Home.md` from guide titles and descriptions when `website/content/docs/` changes on `main`; it can also be run manually.

[Return to getting started →](../getting-started/)
