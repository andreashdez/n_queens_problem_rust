---
title: "GUI guide"
description: "Run the desktop solver, inspect conflicts, compare experiments, and export metrics."
---

Run the native desktop GUI:

```bash
cargo run --release --locked --features gui --bin n_queens_gui
```

The native GUI runs the solver in the background while you inspect the best board and follow its progress. Start with **Quick demo · 8×8**. A solved run may finish at epoch zero if its initial population already includes a solution.

![N-Queens desktop GUI showing a solved 18×18 board, solver settings, run history, and conflict and population-ratio charts.](../../assets/gui-overview.jpg)

*Recommended defaults · 18×18, seed 42: solved at epoch 39. Captured on macOS; runtime varies by machine.*

## Configure a run

1. Choose a preset at the top of the controls.
2. Set the board size and seed.
3. Click **Run solver** and watch the status and charts.

**Advanced settings** contains population/epoch budgets, mutation, parent selection, diversity, and local-search tuning. Hover over a setting for an explanation. Presets preserve the seed and reset all tuning parameters; manual changes are shown as **Custom settings**.

## Inspect the board

Click a queen to inspect its conflicts: the selected queen gets a white ring, attacking queens get square outlines, and their shared diagonals are highlighted. Coordinates and hover details identify each queen; a text explanation lists the attackers. Click the same queen, an empty square, or **Clear selection** to clear it. Selection follows the chosen column as the best board changes during a run, and resets on a new run. Coordinate labels are sampled on large boards to stay readable.

Use **+** / **−**, Ctrl/⌘ + scroll, or pinch to zoom into the board, then drag to pan. **Fit board** resets the view. Zoom is anchored at the pointer for scroll/pinch gestures and supports up to 1,024× magnification; drawing and hit-testing follow the visible board region.

## Compare and repeat runs

**Run history** keeps the last ten completed or cancelled runs in memory for this session, including their original settings, seed, outcome, elapsed time, best board, and sampled metrics. Click a run to inspect it, or choose **A** and **B** to overlay their best-conflict curves. The comparison identifies matching/different seeds and warns when board sizes differ. **Rerun** restores the recorded configuration; **Copy CLI command** copies a reproducible command to run from the project directory.

## Read the charts

Best conflicts tracks the best board found so far. Average conflicts describes the whole current population, so it may rise after a restart even while the best score stays unchanged. [See an annotated example](../algorithm/#an-example-of-a-stalled-run).

Hover over charts for the nearest retained sample's epoch and values. **R** markers show actual soft-restart epochs. Each completed run keeps at most 1,024 metric samples, including the first and final epoch, plus the latest 1,024 restart locations. Live updates can skip intermediate events; completed history is collected in the worker independently of GUI refreshes.

## Export a run

**Export metrics** writes the selected run to a new CSV file using an in-app path field. It refuses to overwrite existing files. The CSV includes original settings, the stop reason and runtime, `sample` rows with retained metrics, and `restart` rows preserving retained restart locations even where metric samples were thinned. `downsampled` identifies sampled histories; sample rows include the cumulative restart count. Use CLI `--metrics-csv` when you need every epoch rather than the GUI's bounded history. Saved runs are cleared when the app closes.

### Which export should I use?

| | GUI **Export metrics** | CLI `--metrics-csv` |
| --- | --- | --- |
| History | At most 1,024 metric samples | Every epoch, including epoch zero |
| Restart events | Separate `restart` rows, at most 1,024 locations | No separate restart-event rows |
| Row format | `sample` and `restart` records | One metric row per epoch |
| Existing file | Refuses overwrite | Replaces the file |
| Best use | Inspect or compare saved GUI runs | Analyze the full epoch history |

These are different CSV schemas. Check the column headers before combining files.

## Stop a run

Cancellation takes effect at an epoch callback; it may take time to finish the current work. Closing the window also requests cancellation. Completed and cancelled runs remain in history while the app is open.

<details>
<summary>How live updates are retained</summary>

The progress queue holds at most one snapshot. Intermediate updates may be skipped to keep memory bounded; the final result is always delivered. Completed history is sampled in the worker independently of screen refreshes. Unexpected worker disconnection appears as an error.

</details>

[Find settings to try →](../tuning/) · [Understand the metrics →](../algorithm/#reading-the-metrics)
