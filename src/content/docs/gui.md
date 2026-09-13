---
title: "GUI guide"
description: "Run the desktop solver, inspect conflicts, compare experiments, and export metrics."
---

Run the native desktop GUI:

```bash
cargo run --release --features gui --bin n_queens_gui
```

The GUI exposes the solver parameters, runs the genetic algorithm on a background thread, supports cancellation, renders the best board, and charts conflict/diversity metrics as epochs complete. Its progress queue holds at most one snapshot, and chart history retains at most 1,024 compact metric points. Intermediate updates may be skipped; the final result is always delivered. Closing the window requests cancellation, and unexpected worker disconnection is shown as an error.

## Configure a run

Choose a preset, board size, and seed at the top of the controls, then click **Run solver**. **Advanced settings** contains population/epoch budgets, mutation, parent selection, diversity, and local-search tuning. Hover over a setting for an explanation. Presets preserve the seed and reset all tuning parameters; manual changes are shown as **Custom settings**.

## Inspect the board

Click a queen to inspect its conflicts: the selected queen gets a white ring, attacking queens get square outlines, and their shared diagonals are highlighted. Coordinates and hover details identify each queen; a text explanation lists the attackers. Click the same queen, an empty square, or **Clear selection** to clear it. Selection follows the chosen column as the best board changes during a run, and resets on a new run. Coordinate labels are sampled on large boards to stay readable.

Use **+** / **−**, Ctrl/⌘ + scroll, or pinch to zoom into the board, then drag to pan. **Fit board** resets the view. Zoom is anchored at the pointer for scroll/pinch gestures and supports up to 1,024× magnification; drawing and hit-testing follow the visible board region.

## Compare and repeat runs

**Run history** keeps the last ten completed or cancelled runs in memory for this session, including their original settings, seed, outcome, elapsed time, best board, and sampled metrics. Click a run to inspect it, or choose **A** and **B** to overlay their best-conflict curves. The comparison identifies matching/different seeds and warns when board sizes differ. **Rerun** restores the recorded configuration; **Copy CLI command** copies a reproducible command to run from the project directory.

## Read the charts

Hover over charts for the nearest retained sample's epoch and values. **R** markers show actual soft-restart epochs. Each completed run keeps at most 1,024 metric samples, including the first and final epoch, plus the latest 1,024 restart locations. Live updates can skip intermediate events; completed history is collected in the worker independently of GUI refreshes.

## Export a run

**Export metrics** writes the selected run to a new CSV file using an in-app path field. It refuses to overwrite existing files. The CSV includes original settings, the stop reason and runtime, `sample` rows with retained metrics, and `restart` rows preserving retained restart locations even where metric samples were thinned. `downsampled` identifies sampled histories; sample rows include the cumulative restart count. Use CLI `--metrics-csv` when you need every epoch rather than the GUI's bounded history. Saved runs are cleared when the app closes.

[Find settings to try →](../tuning/) · [Understand the metrics →](../algorithm/#reading-the-metrics)
