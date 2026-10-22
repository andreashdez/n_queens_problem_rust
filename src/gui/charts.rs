use std::{
    fmt::{Debug, Write as _},
    hash::Hash,
};

use egui_plot::{
    Corner, GridInput, GridMark, HoverPosition, ItemId, ItemIdSet, Legend, Line, LineStyle, Plot,
    PlotMemory, VLine,
};

use eframe::egui::{self, Color32, RichText};

use super::{
    Step,
    palette::Palette,
    worker::{MetricHistory, Progress},
};
use crate::{Solver, ga::EpochMetrics, min_conflicts::PassMetrics};

pub(super) struct ChartMarker {
    pub epoch: u32,
    pub label: String,
    pub color: Color32,
}

pub(super) struct ChartSeries {
    pub label: String,
    pub color: Color32,
    pub values: Vec<(u32, f32)>,
}

/// Grid lines fade in across this pixel spacing; [`linear_marks`] relies on the minimum.
const GRID_SPACING: egui::Rangef = egui::Rangef {
    min: 8.0,
    max: 40.0,
};
/// Keep ticks far enough apart for `egui_plot` to label every one of them.
const X_TICK_POINTS: f64 = 80.0;
const Y_TICK_POINTS: f64 = 32.0;
/// Wide enough for "100%" so linked charts line up.
const Y_AXIS_WIDTH: f32 = 44.0;
/// A restart counts as hovered within this many points of the pointer.
const MIN_PLOT_HEIGHT: f32 = 100.0;
/// The `add_space` and strong title label drawn above each plot.
const CHART_TITLE_HEIGHT: f32 = 4.0 + 18.0;
const MARKER_HOVER_POINTS: f64 = 8.0;

#[allow(
    clippy::cast_precision_loss,
    reason = "Chart coordinates are approximate f32 pixels; stored epochs and metrics retain their integer precision."
)]
pub(super) const fn plot_value(value: u32) -> f32 {
    value as f32
}

/// How a chart maps metric values onto its y axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChartScale {
    /// Conflict counts on a linear axis with whole-number ticks.
    Conflicts,
    /// Conflict counts on a `log10(1 + value)` axis, so small best scores stay
    /// readable next to large population averages.
    LogConflicts,
    /// Ratios in `0..=1`, shown as percentages on a fixed 0–100% axis.
    Percent,
}

impl ChartScale {
    pub const fn conflicts(log: bool) -> Self {
        if log {
            Self::LogConflicts
        } else {
            Self::Conflicts
        }
    }

    fn to_plot(self, value: f32) -> f64 {
        let value = f64::from(value);
        match self {
            Self::Conflicts => value,
            Self::LogConflicts => log_axis(value),
            Self::Percent => value * 100.0,
        }
    }

    fn format_value(self, value: f32) -> String {
        match self {
            Self::Percent => format!("{:.1}%", value * 100.0),
            Self::Conflicts | Self::LogConflicts if value.fract() == 0.0 => format!("{value:.0}"),
            Self::Conflicts | Self::LogConflicts => format!("{value:.2}"),
        }
    }

    fn format_tick(self, mark: GridMark) -> String {
        // Adding zero turns a negative zero into "0" rather than "-0".
        match self {
            Self::Conflicts => format!("{:.0}", mark.value + 0.0),
            Self::LogConflicts => format!(
                "{:.0}",
                (mark.value * std::f64::consts::LN_10).exp_m1().round() + 0.0
            ),
            Self::Percent => format!("{:.0}%", mark.value + 0.0),
        }
    }

    fn grid_marks(self, input: &GridInput) -> Vec<GridMark> {
        match self {
            Self::Conflicts | Self::Percent => linear_marks(input, Y_TICK_POINTS),
            Self::LogConflicts => log_marks(input.bounds),
        }
    }
}

fn log_axis(value: f64) -> f64 {
    value.max(0.0).ln_1p() / std::f64::consts::LN_10
}

/// Smallest 1-2-5 × 10ⁿ step, and at least one unit, that keeps ticks `min_points` apart.
fn nice_step(value_per_point: f64, min_points: f64) -> f64 {
    let target = (value_per_point * min_points).max(1.0);
    if !target.is_finite() {
        return 1.0;
    }
    let magnitude = 10_f64.powf(target.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|multiple| multiple * magnitude)
        .find(|&step| step >= target * (1.0 - 1e-9))
        .unwrap_or(10.0 * magnitude)
}

/// Evenly spaced round ticks covering the visible range.
fn linear_marks(input: &GridInput, min_points: f64) -> Vec<GridMark> {
    let (min, max) = input.bounds;
    let step = nice_step(
        input.base_step_size / f64::from(GRID_SPACING.min),
        min_points,
    );
    if !min.is_finite() || !max.is_finite() || (max - min) / step > 10_000.0 {
        return Vec::new();
    }
    let first = (min / step).ceil();
    // The range check above caps the span at 10,000 steps.
    (0..=10_000_u32)
        .map(|offset| first + f64::from(offset))
        .take_while(|index| index * step <= max)
        .map(|index| GridMark {
            // Adding 0.0 turns -0.0 into 0.0.
            value: index.mul_add(step, 0.0),
            step_size: step,
        })
        .collect()
}

/// Ticks at 0 and 1-2-5 × 10ⁿ on a [`log_axis`]. Zero and powers of ten from 10
/// get a full decade step; the rest use their gap to the previous tick, so
/// `egui_plot` drops crowded intermediate labels first.
fn log_marks((min, max): (f64, f64)) -> Vec<GridMark> {
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    let mut marks = Vec::new();
    let mut previous = 0.0;
    let mut push = |value: f64, major: bool| {
        let y = log_axis(value);
        if (min..=max).contains(&y) {
            marks.push(GridMark {
                value: y,
                step_size: if major { 1.0 } else { y - previous },
            });
        }
        previous = y;
    };
    push(0.0, true);
    let mut decade = 1.0;
    while log_axis(decade) <= max && decade < 1e15 {
        push(decade, decade >= 10.0);
        push(2.0 * decade, false);
        push(5.0 * decade, false);
        decade *= 10.0;
    }
    marks
}

pub(super) fn nearest_chart_sample(values: &[(u32, f32)], epoch: f64) -> Option<(u32, f32)> {
    if !epoch.is_finite()
        || epoch < f64::from(values.first()?.0)
        || epoch > f64::from(values.last()?.0)
    {
        return None;
    }
    let right = values.partition_point(|&(sample, _)| f64::from(sample) < epoch);
    match (
        right.checked_sub(1).and_then(|left| values.get(left)),
        values.get(right),
    ) {
        (Some(&a), Some(&b)) => Some(if epoch - f64::from(a.0) <= f64::from(b.0) - epoch {
            a
        } else {
            b
        }),
        (Some(&a), None) => Some(a),
        (None, Some(&b)) => Some(b),
        (None, None) => None,
    }
}

pub(super) struct Chart<'a> {
    pub title: &'a str,
    /// What one x-axis unit is: "Epoch", "Pass", or both for a mixed comparison.
    pub x_label: &'a str,
    pub height: f32,
    pub scale: ChartScale,
    /// Charts in the same group share their epoch range and hover cursor.
    pub link: Option<egui::Id>,
}

impl Chart<'_> {
    /// `key` identifies the plotted run, so zoom resets when another run is shown.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        key: impl Hash + Debug,
        series: &[ChartSeries],
        markers: &[ChartMarker],
    ) {
        let Self {
            title,
            x_label,
            height,
            scale,
            link,
        } = self;
        let step = x_label.to_lowercase();
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong());

        let id = ui.make_persistent_id((title, key));
        // Last frame's state: legend-hidden items, and the epochs per point for hover tolerance.
        let memory = PlotMemory::load(ui.ctx(), id);
        let tolerance = memory.as_ref().map_or(0.0, |memory| {
            memory.transform().dvalue_dpos()[0].abs() * MARKER_HOVER_POINTS
        });
        let hidden = memory.map(|memory| memory.hidden_items).unwrap_or_default();

        let mut plot = Plot::new(title)
            .id(id)
            .height(height)
            .legend(Legend::default().position(Corner::RightTop))
            // Leave the mouse wheel to the page; zoom with Ctrl/⌘ + scroll or a pinch.
            .allow_scroll(false)
            .allow_zoom([true, false])
            .allow_drag([true, false])
            .grid_spacing(GRID_SPACING)
            .x_grid_spacer(|input| linear_marks(&input, X_TICK_POINTS))
            .y_grid_spacer(move |input| scale.grid_marks(&input))
            .x_axis_formatter(|mark, _| format!("{:.0}", mark.value + 0.0))
            .y_axis_formatter(move |mark, _| scale.format_tick(mark))
            .x_axis_label(x_label)
            // Equal axis widths keep the plot areas of linked charts aligned.
            .y_axis_min_width(Y_AXIS_WIDTH)
            .include_x(0.0)
            .include_y(0.0)
            .label_formatter(move |hover| {
                let (HoverPosition::NearDataPoint { position, .. }
                | HoverPosition::Elsewhere { position }) = hover;
                hover_text(
                    series, markers, &hidden, scale, &step, position.x, tolerance,
                )
            });
        if scale == ChartScale::Percent {
            plot = plot.include_y(100.0);
        }
        if let Some(group) = link {
            plot = plot
                .link_axis(group, [true, false])
                .link_cursor(group, [true, false]);
        }
        plot.show(ui, |plot_ui| {
            for marker in markers {
                plot_ui.vline(
                    VLine::new(marker.label.clone(), f64::from(marker.epoch))
                        .color(marker.color.gamma_multiply(0.6))
                        .style(LineStyle::dashed_loose())
                        .allow_hover(false),
                );
            }
            for line in series {
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
}

/// Every visible series at its retained sample nearest the pointer, plus nearby restarts.
/// `step` names the x-axis unit in lower case.
fn hover_text(
    series: &[ChartSeries],
    markers: &[ChartMarker],
    hidden: &ItemIdSet,
    scale: ChartScale,
    step: &str,
    epoch: f64,
    tolerance: f64,
) -> Option<String> {
    let is_visible = |label: &String| !hidden.contains(&ItemId::new(label));
    let mut text = String::new();
    for line in series.iter().filter(|line| is_visible(&line.label)) {
        if let Some((sample_epoch, value)) = nearest_chart_sample(&line.values, epoch) {
            writeln!(
                text,
                "{} · {step} {sample_epoch}: {}",
                line.label,
                scale.format_value(value)
            )
            .expect("writing to a String cannot fail");
        }
    }
    for marker in markers
        .iter()
        .filter(|marker| {
            is_visible(&marker.label) && (f64::from(marker.epoch) - epoch).abs() <= tolerance
        })
        .take(6)
    {
        writeln!(text, "{} at {step} {}", marker.label, marker.epoch)
            .expect("writing to a String cannot fail");
    }
    let text = text.trim_end();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Plot height that lets `charts` stacked charts fill the rest of `ui`.
pub(super) fn plot_height(ui: &egui::Ui, charts: u8) -> f32 {
    let charts = f32::from(charts);
    // Each chart adds a title row and spacing above its plot.
    let chrome = 2.0f32.mul_add(ui.spacing().item_spacing.y, CHART_TITLE_HEIGHT);
    (charts.mul_add(-chrome, ui.available_height()) / charts).max(MIN_PLOT_HEIGHT)
}

/// The charts for one run: attacking pairs and population ratios sharing an
/// epoch axis for the GA, or attacking pairs by pass for min-conflicts.
pub(super) fn draw_metric_charts(
    ui: &mut egui::Ui,
    history: &MetricHistory,
    key: impl Hash + Debug + Copy,
    log_conflicts: bool,
) {
    let palette = Palette::of(ui.ctx());
    let Some(first) = history.points.first() else {
        ui.label("Charts appear after the first progress report.");
        return;
    };
    let solver = match first {
        Progress::Ga(_) => Solver::Ga,
        Progress::MinConflicts(_) => Solver::MinConflicts,
    };
    let markers: Vec<_> = history
        .restart_epochs
        .iter()
        .map(|&epoch| ChartMarker {
            epoch,
            label: match solver {
                Solver::Ga => "Soft restart",
                Solver::MinConflicts => "Restart",
            }
            .into(),
            color: palette.restart(),
        })
        .collect();
    if let Some(latest) = history.points.last()
        && latest.restart_count() as usize > markers.len()
    {
        ui.weak(format!(
            "Showing {} of {} restart locations",
            markers.len(),
            latest.restart_count()
        ));
    }

    let best_board = ChartSeries {
        label: "Best board".into(),
        color: palette.best_conflicts(),
        values: history
            .points
            .iter()
            .map(|point| (point.step(), plot_value(point.best_conflicts_sum())))
            .collect(),
    };
    let x_label = Step::of(solver).title;
    match solver {
        Solver::Ga => draw_ga_charts(ui, history, best_board, &markers, key, log_conflicts),
        Solver::MinConflicts => {
            let current_board = ChartSeries {
                label: "Current board".into(),
                color: palette.average_conflicts,
                values: pass_metrics(history)
                    .map(|metrics| (metrics.pass(), plot_value(metrics.current_conflicts_sum())))
                    .collect(),
            };
            Chart {
                title: "Attacking pairs",
                x_label,
                height: plot_height(ui, 1),
                scale: ChartScale::conflicts(log_conflicts),
                link: None,
            }
            .show(ui, key, &[best_board, current_board], &markers);
        }
    }
}

fn epoch_metrics(history: &MetricHistory) -> impl Iterator<Item = &EpochMetrics> {
    history.points.iter().filter_map(|point| match point {
        Progress::Ga(metrics) => Some(metrics),
        Progress::MinConflicts(_) => None,
    })
}

fn pass_metrics(history: &MetricHistory) -> impl Iterator<Item = &PassMetrics> {
    history.points.iter().filter_map(|point| match point {
        Progress::MinConflicts(metrics) => Some(metrics),
        Progress::Ga(_) => None,
    })
}

fn draw_ga_charts(
    ui: &mut egui::Ui,
    history: &MetricHistory,
    best_board: ChartSeries,
    markers: &[ChartMarker],
    key: impl Hash + Debug + Copy,
    log_conflicts: bool,
) {
    let palette = Palette::of(ui.ctx());
    let height = plot_height(ui, 2);
    let link = Some(ui.make_persistent_id(("metric_charts", key)));
    let x_label = Step::of(Solver::Ga).title;
    let series = |label: &str, color, value: fn(&EpochMetrics) -> f32| ChartSeries {
        label: label.into(),
        color,
        values: epoch_metrics(history)
            .map(|metrics| (metrics.epoch(), value(metrics)))
            .collect(),
    };
    let conflicts = [
        best_board,
        series(
            "Population average",
            palette.average_conflicts,
            EpochMetrics::average_conflicts_sum,
        ),
    ];
    Chart {
        title: "Attacking pairs",
        x_label,
        height,
        scale: ChartScale::conflicts(log_conflicts),
        link,
    }
    .show(ui, key, &conflicts, markers);

    let rates = [
        series(
            "Diversity",
            palette.diversity,
            EpochMetrics::diversity_ratio,
        ),
        series(
            "Mutation rate",
            palette.mutation_rate,
            EpochMetrics::mutation_rate,
        ),
        series(
            "Elite fraction",
            palette.elite_fraction,
            EpochMetrics::elite_ratio,
        ),
    ];
    Chart {
        title: "Population ratios",
        x_label,
        height,
        scale: ChartScale::Percent,
        link,
    }
    .show(ui, key, &rates, markers);
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Tick steps and scaled values here are exactly representable."
)]
mod tests {
    use super::*;

    fn input(bounds: (f64, f64), value_per_point: f64) -> GridInput {
        GridInput {
            bounds,
            base_step_size: value_per_point * f64::from(GRID_SPACING.min),
        }
    }

    fn values(marks: &[GridMark]) -> Vec<f64> {
        marks.iter().map(|mark| mark.value).collect()
    }

    #[test]
    fn chart_hover_uses_nearest_sample_and_never_extrapolates() {
        let points = [(0, 10.0), (10, 5.0), (20, 0.0)];
        assert_eq!(nearest_chart_sample(&points, 6.0), Some((10, 5.0)));
        assert_eq!(nearest_chart_sample(&points, 5.0), Some((0, 10.0)));
        assert_eq!(nearest_chart_sample(&points, 20.0), Some((20, 0.0)));
        assert_eq!(nearest_chart_sample(&points, 21.0), None);
        assert_eq!(nearest_chart_sample(&points, -1.0), None);
        assert_eq!(nearest_chart_sample(&points, f64::NAN), None);
        assert_eq!(nearest_chart_sample(&[], 0.0), None);
        assert_eq!(nearest_chart_sample(&[(0, 1.0)], 0.0), Some((0, 1.0)));
    }

    #[test]
    fn linear_ticks_are_round_whole_numbers() {
        // The old chart split 0..=11.7 into quarters: 2.9, 5.8, 8.8.
        assert_eq!(
            values(&linear_marks(&input((-0.6, 12.3), 0.1), 32.0)),
            [0.0, 5.0, 10.0]
        );
        assert_eq!(
            values(&linear_marks(&input((0.0, 39.0), 0.05), 80.0)),
            [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0]
        );
        // Fine zoom never produces fractional conflicts or epochs.
        assert_eq!(
            values(&linear_marks(&input((0.0, 3.0), 0.001), 32.0)),
            [0.0, 1.0, 2.0, 3.0]
        );
        assert_eq!(nice_step(3.0, 32.0), 100.0);
        assert_eq!(nice_step(f64::NAN, 32.0), 1.0);
        assert_eq!(
            values(&linear_marks(&input((f64::NEG_INFINITY, 1.0), 0.1), 32.0)),
            Vec::<f64>::new()
        );
        assert!(
            linear_marks(&input((-0.5, 0.0), 0.1), 32.0)
                .iter()
                .all(|mark| mark.value.is_sign_positive())
        );
    }

    #[test]
    fn log_ticks_follow_one_two_five_and_label_original_counts() {
        let marks = log_marks((-0.1, log_axis(600.0)));
        let labels: Vec<_> = marks
            .iter()
            .map(|&mark| ChartScale::LogConflicts.format_tick(mark))
            .collect();
        assert_eq!(
            labels,
            ["0", "1", "2", "5", "10", "20", "50", "100", "200", "500"]
        );
        let majors: Vec<_> = marks
            .iter()
            .filter(|mark| mark.step_size == 1.0)
            .map(|&mark| ChartScale::LogConflicts.format_tick(mark))
            .collect();
        assert_eq!(majors, ["0", "10", "100"]);
        assert!(marks.iter().all(|mark| mark.step_size > 0.0));
        assert_eq!(values(&log_marks((f64::NAN, 1.0))), Vec::<f64>::new());
    }

    #[test]
    fn values_and_ticks_format_for_their_scale() {
        assert_eq!(ChartScale::Percent.format_value(0.636), "63.6%");
        assert_eq!(ChartScale::Conflicts.format_value(4.0), "4");
        assert_eq!(ChartScale::LogConflicts.format_value(4.126), "4.13");
        let tick = |value| GridMark {
            value,
            step_size: 1.0,
        };
        assert_eq!(ChartScale::Percent.format_tick(tick(-0.0)), "0%");
        assert_eq!(ChartScale::Percent.format_tick(tick(75.0)), "75%");
        assert_eq!(ChartScale::Conflicts.format_tick(tick(-0.0)), "0");
        assert_eq!(ChartScale::Percent.to_plot(0.25), 25.0);
        assert!((ChartScale::LogConflicts.to_plot(9.0) - 1.0).abs() < 1e-12);
        assert_eq!(ChartScale::LogConflicts.to_plot(-1.0), 0.0);
    }

    #[test]
    fn hover_lists_visible_series_and_nearby_restarts() {
        let series = [
            ChartSeries {
                label: "Best".into(),
                color: Color32::WHITE,
                values: vec![(0, 3.0), (10, 1.0)],
            },
            ChartSeries {
                label: "Ratio".into(),
                color: Color32::WHITE,
                values: vec![(0, 0.5), (10, 0.25)],
            },
        ];
        let markers = [ChartMarker {
            epoch: 9,
            label: "Soft restart".into(),
            color: Color32::WHITE,
        }];
        let text = hover_text(
            &series,
            &markers,
            &ItemIdSet::default(),
            ChartScale::Conflicts,
            "epoch",
            8.0,
            2.0,
        )
        .unwrap();
        assert_eq!(
            text,
            "Best · epoch 10: 1\nRatio · epoch 10: 0.25\nSoft restart at epoch 9"
        );
        let hidden: ItemIdSet = std::iter::once(ItemId::new(&series[0].label)).collect();
        let text = hover_text(
            &series,
            &markers,
            &hidden,
            ChartScale::Percent,
            "pass",
            8.0,
            0.5,
        );
        assert_eq!(text.as_deref(), Some("Ratio · pass 10: 25.0%"));
        assert_eq!(
            hover_text(
                &series,
                &[],
                &ItemIdSet::default(),
                ChartScale::Conflicts,
                "epoch",
                50.0,
                1.0
            ),
            None
        );
    }
}
