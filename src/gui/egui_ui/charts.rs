//! Plots charts with `egui_plot`.

use std::{fmt::Debug, hash::Hash};

use egui_plot::{
    Corner, GridInput, GridMark, HoverPosition, ItemId, Legend, Line, LineStyle, Plot, PlotMemory,
    VLine,
};

use eframe::egui::{self, Color32, RichText};

use super::palette;
use crate::gui::{
    charts::{
        ChartSpec, NO_COMPARISON_TEXT, NO_METRICS_TEXT, Tick, X_TICK_POINTS, comparison,
        hover_text, linear_ticks, metric_charts,
    },
    history::RunArchive,
    worker::MetricHistory,
};

/// Grid lines fade in across this pixel spacing; tick spacing relies on the minimum.
const GRID_SPACING: egui::Rangef = egui::Rangef {
    min: 8.0,
    max: 40.0,
};
/// Wide enough for "100%" so linked charts line up.
const Y_AXIS_WIDTH: f32 = 44.0;
const MIN_PLOT_HEIGHT: f32 = 100.0;
/// The `add_space` and strong title label drawn above each plot.
const CHART_TITLE_HEIGHT: f32 = 4.0 + 18.0;
/// A restart counts as hovered within this many points of the pointer.
const MARKER_HOVER_POINTS: f64 = 8.0;

/// Plot values per screen point, from the step `egui_plot` would put between grid lines.
fn value_per_point(input: &GridInput) -> f64 {
    input.base_step_size / f64::from(GRID_SPACING.min)
}

fn grid_marks(ticks: Vec<Tick>) -> Vec<GridMark> {
    ticks
        .into_iter()
        .map(|tick| GridMark {
            value: tick.value,
            step_size: tick.step_size,
        })
        .collect()
}

/// Plots `chart`, `height` points tall. `key` identifies the plotted run, so zoom
/// resets when another run is shown. Charts with the same `link` share their x
/// range and hover cursor.
fn show_chart(
    ui: &mut egui::Ui,
    chart: &ChartSpec,
    height: f32,
    key: impl Hash + Debug,
    link: Option<egui::Id>,
) {
    let scale = chart.scale;
    ui.add_space(4.0);
    ui.label(RichText::new(chart.title).strong());

    let id = ui.make_persistent_id((chart.title, key));
    // Last frame's state: legend-hidden items, and the epochs per point for hover tolerance.
    let memory = PlotMemory::load(ui.ctx(), id);
    let tolerance = memory.as_ref().map_or(0.0, |memory| {
        memory.transform().dvalue_dpos()[0].abs() * MARKER_HOVER_POINTS
    });
    let hidden = memory.map(|memory| memory.hidden_items).unwrap_or_default();
    let (include_low, include_high) = scale.included_y();

    let mut plot = Plot::new(chart.title)
        .id(id)
        .height(height)
        .legend(Legend::default().position(Corner::RightTop))
        // Leave the mouse wheel to the page; zoom with Ctrl/⌘ + scroll or a pinch.
        .allow_scroll(false)
        .allow_zoom([true, false])
        .allow_drag([true, false])
        .grid_spacing(GRID_SPACING)
        .x_grid_spacer(|input| {
            grid_marks(linear_ticks(
                input.bounds,
                value_per_point(&input),
                X_TICK_POINTS,
            ))
        })
        .y_grid_spacer(move |input| grid_marks(scale.ticks(input.bounds, value_per_point(&input))))
        .x_axis_formatter(|mark, _| format!("{:.0}", mark.value + 0.0))
        .y_axis_formatter(move |mark, _| scale.format_tick(mark.value))
        .x_axis_label(chart.x_label)
        // Equal axis widths keep the plot areas of linked charts aligned.
        .y_axis_min_width(Y_AXIS_WIDTH)
        .include_x(0.0)
        .include_y(include_low)
        .include_y(include_high)
        .label_formatter(move |hover| {
            let (HoverPosition::NearDataPoint { position, .. }
            | HoverPosition::Elsewhere { position }) = hover;
            hover_text(
                chart,
                |label| !hidden.contains(&ItemId::new(label)),
                position.x,
                tolerance,
            )
        });
    if let Some(group) = link {
        plot = plot
            .link_axis(group, [true, false])
            .link_cursor(group, [true, false]);
    }
    plot.show(ui, |plot_ui| {
        for marker in &chart.markers {
            plot_ui.vline(
                VLine::new(marker.label.clone(), f64::from(marker.epoch))
                    .color(Color32::from(marker.color).gamma_multiply(0.6))
                    .style(LineStyle::dashed_loose())
                    .allow_hover(false),
            );
        }
        for line in &chart.series {
            plot_ui.line(
                Line::new(
                    line.label.clone(),
                    line.values
                        .iter()
                        .map(|&(epoch, value)| [f64::from(epoch), scale.to_plot(value)])
                        .collect::<Vec<_>>(),
                )
                .color(line.color)
                .width(2.0),
            );
        }
    });
}

/// Plot height that lets `charts` stacked charts fill the rest of `ui`.
fn plot_height(ui: &egui::Ui, charts: usize) -> f32 {
    let charts = f32::from(u8::try_from(charts).unwrap_or(u8::MAX).max(1));
    // Each chart adds a title row and spacing above its plot.
    let chrome = 2.0f32.mul_add(ui.spacing().item_spacing.y, CHART_TITLE_HEIGHT);
    (charts.mul_add(-chrome, ui.available_height()) / charts).max(MIN_PLOT_HEIGHT)
}

/// The charts for one run, stacked and linked.
pub(super) fn draw_metric_charts(
    ui: &mut egui::Ui,
    history: &MetricHistory,
    key: impl Hash + Debug + Copy,
    log_conflicts: bool,
) {
    let Some(charts) = metric_charts(history, palette(ui.ctx()), log_conflicts) else {
        ui.label(NO_METRICS_TEXT);
        return;
    };
    if let Some(note) = charts.note {
        ui.weak(note);
    }
    let height = plot_height(ui, charts.charts.len());
    let link = (charts.charts.len() > 1).then(|| ui.make_persistent_id(("metric_charts", key)));
    for chart in &charts.charts {
        show_chart(ui, chart, height, key, link);
    }
}

/// The convergence of the runs marked A and B.
pub(super) fn draw_comparison(ui: &mut egui::Ui, archive: &RunArchive, log_conflicts: bool) {
    let Some([a, b]) = archive.comparison_pair() else {
        ui.label(NO_COMPARISON_TEXT);
        return;
    };
    let comparison = comparison(a, b, palette(ui.ctx()), log_conflicts);
    ui.label(comparison.context);
    ui.weak(comparison.caption);
    let height = plot_height(ui, 1);
    show_chart(ui, &comparison.chart, height, (a.id, b.id), None);
}
