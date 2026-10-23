---
title: "Development"
description: "Run the Rust checks, build the web GUI, work on the Astro documentation site, and release the macOS app."
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

Then `trunk serve`, run from the repository root, builds `website/gui/index.html` and serves it at <http://127.0.0.1:8080/>, rebuilding on changes. Add `--release` for realistic solver speed. Without threads, the GUI steps the solver for a slice of each frame (`SteppedWorker` in `src/gui/worker.rs`) instead of running it on a background thread. To lint that code path, run clippy for the web target too:

```bash
cargo clippy --lib --bins --features gui --target wasm32-unknown-unknown -- \
  -D clippy::pedantic -D clippy::nursery -D warnings
```

## Work on the docs

The site uses Astro and Starlight. With Node.js 24 LTS (Astro needs 22.23.4 or newer), npm, Python 3.10+, and Rust installed, run from `website/`:

```bash
cd website
npm ci
npm run check
npm run build:gui
npm run build
npm run check:links
npm run test:docs
npm run check:examples
```

`build:gui` builds the web GUI into `website/public/demo/`, which the site build copies to `/demo/`. It needs the Trunk setup above, and `check:links` fails on the links to the demo without it. `check:links` scans the built HTML for missing pages, assets, and section anchors, including legacy bookmarks. It uses the build's canonical URL to respect the GitHub Pages base path. `test:docs` checks that these validators catch broken fixtures.

`check:examples` runs only the small CLI examples marked with `docs-check` comments and compares their JSON fields with the nearby `docs-result` blocks. It also compares the CLI reference's flags, aliases, and defaults against `--help`. Runtime is deliberately excluded. No native GUI is opened by these checks.

When changing CLI defaults or examples, update the reference and expected output in the same change. When adding a guide, update the sidebar and `website/scripts/wiki-home.py` navigation list. CI builds and checks both the site root and a repository subpath, and the Pages workflow checks links again before uploading.

For local editing, `npm run dev` in `website/` starts the documentation server. The site is a self-contained Astro project in `website/`, kept separate from the Rust crate: edit pages in `website/src/content/docs/`, the sidebar in `website/astro.config.mjs`, and the theme in `website/src/styles/docs.css`. The documentation checks live in `website/scripts/`.

GitHub Actions builds the site with the repository's Pages origin and base path. Pushes to `main` that change documentation, site assets, or the Rust sources behind the web GUI trigger deployment. The GitHub Wiki Home links to the published guides; the Pages site is the full documentation.

## Update the GUI screenshot

The README, homepage, and GUI guide share `website/src/assets/gui-overview.png`. The current image was captured on macOS from revision `674338b` using **Recommended defaults · 18×18**, seed **42**, which solved at epoch **39**.

1. Build and launch the current GUI with `cargo run --release --locked --features gui --bin n_queens_gui`.
2. Select the recommended preset, set seed 42, and click **Run solver**.
3. Resize the window and, if needed, reduce the interface scale with Ctrl/⌘ + **−** so the board and both charts are fully visible. Capture only the application window.
4. Replace the shared image and update the captions and this capture record if the preset, seed, or outcome changes. Keep the filename extension consistent with the image format.
5. Run the documentation build and link checks above.

The screenshot records a real run. Its elapsed time is illustrative, not a benchmark.

## Update the app icon

`assets/app-icon.svg` is the source for the logo, drawn with the colors and queen of the board on the docs homepage. The desktop GUI embeds `assets/app-icon.png` as its window and Dock icon when run with Cargo. The macOS app bundle uses `assets/app-icon.icon` instead, made with Icon Composer from the same art. The web GUI and the docs site use `website/public/favicon.svg`, a simpler queen tile that stays legible at small sizes. After changing the SVG, update the Icon Composer icon to match and render the PNG again:

```bash
magick -background none assets/app-icon.svg -resize 512x512 -depth 8 -strip assets/app-icon.png
```

## Release the macOS app

`scripts/bundle-macos.sh` builds the desktop GUI for Apple Silicon and Intel, packages it as `target/macos/N-Queens Solver.app` with the Icon Composer icon, and zips it. It needs macOS with Xcode 26 or later.

To publish a release, set `version` in `Cargo.toml`, then push a matching tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The Release workflow checks that the tag matches the crate version, builds the app on a macOS runner, and attaches `N-Queens-Solver-<version>-macos.zip` to the GitHub release for that tag, creating the release if needed. Running the workflow manually builds the same zip as a workflow artifact without publishing anything.

The app is signed ad hoc, not with an Apple Developer ID, so macOS blocks the first launch of a downloaded copy. Users open it once from **System Settings → Privacy & Security → Open Anyway**.

## GitHub Wiki sync

Enable **Wikis** under the repository's **Settings → General → Features**. The `.github/workflows/wiki-sync.yml` workflow regenerates Wiki `Home.md` from guide titles and descriptions when `website/src/content/docs/` changes on `main`; it can also be run manually.

[Return to getting started →](../getting-started/)
