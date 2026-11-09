//! What the charts show: their series, restart markers, axis scales, ticks and
//! hover text. Drawing and interaction are left to the toolkit.

use std::fmt::Write as _;

use super::{
    Step,
    history::{SavedRun, comparison_context},
    palette::{Palette, Rgba},
    worker::{MetricHistory, Progress},
};
use crate::{Solver, ga::EpochMetrics, min_conflicts::PassMetrics};

pub(super) const NO_METRICS_TEXT: &str = "Charts appear after the first progress report.";
pub(super) const NO_COMPARISON_TEXT: &str =
    "Mark one run A and another B in run history to compare their convergence.";

pub(super) struct ChartMarker {
    pub epoch: u32,
    pub label: String,
    pub color: Rgba,
}

pub(super) struct ChartSeries {
    pub label: String,
    pub color: Rgba,
    pub values: Vec<(u32, f32)>,
}

/// One chart: its title, axes and what it plots.
pub(super) struct ChartSpec {
    pub title: &'static str,
    /// What one x-axis unit is: "Epoch", "Pass", or both for a mixed comparison.
    pub x_label: &'static str,
    pub scale: ChartScale,
    pub series: Vec<ChartSeries>,
    pub markers: Vec<ChartMarker>,
}

/// The charts for one run.
pub(super) struct MetricCharts {
    /// Set when the history kept fewer restart locations than the run made.
    pub note: Option<String>,
    /// Charts stacked top to bottom. When there are several, they share their
    /// x range and hover cursor.
    pub charts: Vec<ChartSpec>,
}

/// Two runs' convergence, side by side.
pub(super) struct Comparison {
    /// How far the two runs can be compared.
    pub context: String,
    pub caption: String,
    pub chart: ChartSpec,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Chart coordinates are approximate f32 pixels; stored epochs and metrics retain their integer precision."
)]
pub(super) const fn plot_value(value: u32) -> f32 {
    value as f32
}

/// A tick on a chart axis, at `value` in plot coordinates. `step_size` is the gap
/// it stands for, so a toolkit can drop crowded minor ticks first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tick {
    pub value: f64,
    pub step_size: f64,
}

/// Points between x-axis ticks, so every tick has room for its label.
pub(super) const X_TICK_POINTS: f64 = 80.0;
/// Points between y-axis ticks.
pub(super) const Y_TICK_POINTS: f64 = 32.0;

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

    pub(super) fn to_plot(self, value: f32) -> f64 {
        let value = f64::from(value);
        match self {
            Self::Conflicts => value,
            Self::LogConflicts => log_axis(value),
            Self::Percent => value * 100.0,
        }
    }

    pub(super) fn format_value(self, value: f32) -> String {
        match self {
            Self::Percent => format!("{:.1}%", value * 100.0),
            Self::Conflicts | Self::LogConflicts if value.fract() == 0.0 => format!("{value:.0}"),
            Self::Conflicts | Self::LogConflicts => format!("{value:.2}"),
        }
    }

    /// The label for a tick at `value` in plot coordinates.
    pub(super) fn format_tick(self, value: f64) -> String {
        // Adding zero turns a negative zero into "0" rather than "-0".
        match self {
            Self::Conflicts => format!("{:.0}", value + 0.0),
            Self::LogConflicts => format!(
                "{:.0}",
                (value * std::f64::consts::LN_10).exp_m1().round() + 0.0
            ),
            Self::Percent => format!("{:.0}%", value + 0.0),
        }
    }

    /// Y-axis ticks for the visible `bounds`, in plot coordinates.
    pub(super) fn ticks(self, bounds: (f64, f64), value_per_point: f64) -> Vec<Tick> {
        match self {
            Self::Conflicts | Self::Percent => linear_ticks(bounds, value_per_point, Y_TICK_POINTS),
            Self::LogConflicts => log_ticks(bounds),
        }
    }

    /// The y range that must stay in view, in plot coordinates.
    pub(super) const fn included_y(self) -> (f64, f64) {
        match self {
            Self::Conflicts | Self::LogConflicts => (0.0, 0.0),
            Self::Percent => (0.0, 100.0),
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

/// Evenly spaced round ticks covering `(min, max)`, at least `min_points` apart.
pub(super) fn linear_ticks(
    (min, max): (f64, f64),
    value_per_point: f64,
    min_points: f64,
) -> Vec<Tick> {
    let step = nice_step(value_per_point, min_points);
    if !min.is_finite() || !max.is_finite() || (max - min) / step > 10_000.0 {
        return Vec::new();
    }
    let first = (min / step).ceil();
    // The range check above caps the span at 10,000 steps.
    (0..=10_000_u32)
        .map(|offset| first + f64::from(offset))
        .take_while(|index| index * step <= max)
        .map(|index| Tick {
            // Adding 0.0 turns -0.0 into 0.0.
            value: index.mul_add(step, 0.0),
            step_size: step,
        })
        .collect()
}

/// Ticks at 0 and 1-2-5 × 10ⁿ on a [`log_axis`]. Zero and powers of ten from 10
/// get a full decade step; the rest use their gap to the previous tick, so
/// crowded intermediate labels drop first.
fn log_ticks((min, max): (f64, f64)) -> Vec<Tick> {
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    let mut ticks = Vec::new();
    let mut previous = 0.0;
    let mut push = |value: f64, major: bool| {
        let y = log_axis(value);
        if (min..=max).contains(&y) {
            ticks.push(Tick {
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
    ticks
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

/// Every visible series at its retained sample nearest `epoch`, plus restarts
/// within `tolerance` of it. `x_label` names the x-axis unit.
pub(super) fn hover_text(
    chart: &ChartSpec,
    is_visible: impl Fn(&str) -> bool,
    epoch: f64,
    tolerance: f64,
) -> Option<String> {
    let step = chart.x_label.to_lowercase();
    let mut text = String::new();
    for line in chart.series.iter().filter(|line| is_visible(&line.label)) {
        if let Some((sample_epoch, value)) = nearest_chart_sample(&line.values, epoch) {
            writeln!(
                text,
                "{} · {step} {sample_epoch}: {}",
                line.label,
                chart.scale.format_value(value)
            )
            .expect("writing to a String cannot fail");
        }
    }
    for marker in chart
        .markers
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

/// The charts for one run: attacking pairs and population ratios sharing an
/// epoch axis for the GA, or attacking pairs by pass for min-conflicts. `None`
/// until the first progress report.
pub(super) fn metric_charts(
    history: &MetricHistory,
    palette: &Palette,
    log_conflicts: bool,
) -> Option<MetricCharts> {
    let solver = match history.points.first()? {
        Progress::Ga(_) => Solver::Ga,
        Progress::MinConflicts(_) => Solver::MinConflicts,
    };
    let markers = || restart_markers(history, solver, palette);
    let note = history
        .points
        .last()
        .map(Progress::restart_count)
        .filter(|&count| count as usize > history.restart_epochs.len())
        .map(|count| {
            format!(
                "Showing {} of {count} restart locations",
                history.restart_epochs.len()
            )
        });

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
    let conflicts = ChartScale::conflicts(log_conflicts);
    let charts = match solver {
        Solver::Ga => ga_charts(history, palette, conflicts, best_board),
        Solver::MinConflicts => vec![ChartSpec {
            title: "Attacking pairs",
            x_label,
            scale: conflicts,
            series: vec![
                best_board,
                ChartSeries {
                    label: "Current board".into(),
                    color: palette.average_conflicts,
                    values: pass_metrics(history)
                        .map(|metrics| {
                            (metrics.pass(), plot_value(metrics.current_conflicts_sum()))
                        })
                        .collect(),
                },
            ],
            markers: markers(),
        }],
    };
    Some(MetricCharts { note, charts })
}

/// Where the run restarted, as dashed chart markers.
fn restart_markers(history: &MetricHistory, solver: Solver, palette: &Palette) -> Vec<ChartMarker> {
    history
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
        .collect()
}

/// Attacking pairs and population ratios, sharing an epoch axis.
fn ga_charts(
    history: &MetricHistory,
    palette: &Palette,
    conflicts: ChartScale,
    best_board: ChartSeries,
) -> Vec<ChartSpec> {
    let series = |label: &str, color, value: fn(&EpochMetrics) -> f32| ChartSeries {
        label: label.into(),
        color,
        values: epoch_metrics(history)
            .map(|metrics| (metrics.epoch(), value(metrics)))
            .collect(),
    };
    vec![
        ChartSpec {
            title: "Attacking pairs",
            x_label: Step::of(Solver::Ga).title,
            scale: conflicts,
            series: vec![
                best_board,
                series(
                    "Population average",
                    palette.average_conflicts,
                    EpochMetrics::average_conflicts_sum,
                ),
            ],
            markers: restart_markers(history, Solver::Ga, palette),
        },
        ChartSpec {
            title: "Population ratios",
            x_label: Step::of(Solver::Ga).title,
            scale: ChartScale::Percent,
            series: vec![
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
            ],
            markers: restart_markers(history, Solver::Ga, palette),
        },
    ]
}

/// Best-board attacking pairs of runs `a` and `b`, with their restarts.
pub(super) fn comparison(
    a: &SavedRun,
    b: &SavedRun,
    palette: &Palette,
    log_conflicts: bool,
) -> Comparison {
    let (solver_a, solver_b) = (a.result.config.solver, b.result.config.solver);
    let x_label = if solver_a == solver_b {
        Step::of(solver_a).title
    } else {
        "Epoch / pass"
    };
    let colors = palette.compare();
    let runs = [a, b];
    let series = runs
        .iter()
        .enumerate()
        .map(|(index, run)| ChartSeries {
            label: format!("{} · Run #{}", if index == 0 { "A" } else { "B" }, run.id),
            color: colors[index],
            values: run
                .result
                .history
                .points
                .iter()
                .map(|point| (point.step(), plot_value(point.best_conflicts_sum())))
                .collect(),
        })
        .collect();
    let markers = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| {
            run.result
                .history
                .restart_epochs
                .iter()
                .map(move |&epoch| ChartMarker {
                    epoch,
                    label: format!("Run #{} restart", run.id),
                    color: colors[index],
                })
        })
        .collect();
    Comparison {
        context: comparison_context(&a.result.config, &b.result.config),
        caption: format!(
            "Best-board attacking pairs by {} · sampled points · dashed lines mark restarts",
            x_label.to_lowercase()
        ),
        chart: ChartSpec {
            title: "Convergence",
            x_label,
            scale: ChartScale::conflicts(log_conflicts),
            series,
            markers,
        },
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

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Tick steps and scaled values here are exactly representable."
)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::gui::{config::GuiConfig, worker::tests::finished_run};

    fn values(ticks: &[Tick]) -> Vec<f64> {
        ticks.iter().map(|tick| tick.value).collect()
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
            values(&linear_ticks((-0.6, 12.3), 0.1, 32.0)),
            [0.0, 5.0, 10.0]
        );
        assert_eq!(
            values(&linear_ticks((0.0, 39.0), 0.05, 80.0)),
            [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0]
        );
        // Fine zoom never produces fractional conflicts or epochs.
        assert_eq!(
            values(&linear_ticks((0.0, 3.0), 0.001, 32.0)),
            [0.0, 1.0, 2.0, 3.0]
        );
        assert_eq!(nice_step(3.0, 32.0), 100.0);
        assert_eq!(nice_step(f64::NAN, 32.0), 1.0);
        assert_eq!(
            values(&linear_ticks((f64::NEG_INFINITY, 1.0), 0.1, 32.0)),
            Vec::<f64>::new()
        );
        assert!(
            linear_ticks((-0.5, 0.0), 0.1, 32.0)
                .iter()
                .all(|tick| tick.value.is_sign_positive())
        );
    }

    #[test]
    fn log_ticks_follow_one_two_five_and_label_original_counts() {
        let ticks = log_ticks((-0.1, log_axis(600.0)));
        let labels: Vec<_> = ticks
            .iter()
            .map(|tick| ChartScale::LogConflicts.format_tick(tick.value))
            .collect();
        assert_eq!(
            labels,
            ["0", "1", "2", "5", "10", "20", "50", "100", "200", "500"]
        );
        let majors: Vec<_> = ticks
            .iter()
            .filter(|tick| tick.step_size == 1.0)
            .map(|tick| ChartScale::LogConflicts.format_tick(tick.value))
            .collect();
        assert_eq!(majors, ["0", "10", "100"]);
        assert!(ticks.iter().all(|tick| tick.step_size > 0.0));
        assert_eq!(values(&log_ticks((f64::NAN, 1.0))), Vec::<f64>::new());
        assert_eq!(
            ChartScale::LogConflicts.ticks((-0.1, log_axis(600.0)), 1.0),
            ticks
        );
    }

    #[test]
    fn values_and_ticks_format_for_their_scale() {
        assert_eq!(ChartScale::Percent.format_value(0.636), "63.6%");
        assert_eq!(ChartScale::Conflicts.format_value(4.0), "4");
        assert_eq!(ChartScale::LogConflicts.format_value(4.126), "4.13");
        assert_eq!(ChartScale::Percent.format_tick(-0.0), "0%");
        assert_eq!(ChartScale::Percent.format_tick(75.0), "75%");
        assert_eq!(ChartScale::Conflicts.format_tick(-0.0), "0");
        assert_eq!(ChartScale::Percent.to_plot(0.25), 25.0);
        assert!((ChartScale::LogConflicts.to_plot(9.0) - 1.0).abs() < 1e-12);
        assert_eq!(ChartScale::LogConflicts.to_plot(-1.0), 0.0);
        assert_eq!(ChartScale::Percent.included_y(), (0.0, 100.0));
    }

    #[test]
    fn hover_lists_visible_series_and_nearby_restarts() {
        let mut chart = ChartSpec {
            title: "Test",
            x_label: "Epoch",
            scale: ChartScale::Conflicts,
            series: vec![
                ChartSeries {
                    label: "Best".into(),
                    color: Rgba::WHITE,
                    values: vec![(0, 3.0), (10, 1.0)],
                },
                ChartSeries {
                    label: "Ratio".into(),
                    color: Rgba::WHITE,
                    values: vec![(0, 0.5), (10, 0.25)],
                },
            ],
            markers: vec![ChartMarker {
                epoch: 9,
                label: "Soft restart".into(),
                color: Rgba::WHITE,
            }],
        };
        assert_eq!(
            hover_text(&chart, |_| true, 8.0, 2.0).unwrap(),
            "Best · epoch 10: 1\nRatio · epoch 10: 0.25\nSoft restart at epoch 9"
        );
        chart.scale = ChartScale::Percent;
        chart.x_label = "Pass";
        assert_eq!(
            hover_text(&chart, |label| label != "Best", 8.0, 0.5).as_deref(),
            Some("Ratio · pass 10: 25.0%")
        );
        chart.markers.clear();
        assert_eq!(hover_text(&chart, |_| true, 50.0, 1.0), None);
    }

    #[test]
    fn ga_and_min_conflicts_runs_get_their_own_charts() {
        let palette = &Palette::DARK;
        assert!(metric_charts(&MetricHistory::default(), palette, true).is_none());

        let ga = finished_run(GuiConfig {
            board_size: 3,
            population_size: 1,
            max_epochs: 5_000,
            allow_unsolvable: true,
            ..Default::default()
        });
        let charts = metric_charts(&ga.history, palette, true).unwrap();
        let titles: Vec<_> = charts.charts.iter().map(|chart| chart.title).collect();
        assert_eq!(titles, ["Attacking pairs", "Population ratios"]);
        assert_eq!(charts.charts[0].scale, ChartScale::LogConflicts);
        assert_eq!(charts.charts[1].scale, ChartScale::Percent);
        assert_eq!(charts.charts[0].x_label, "Epoch");
        assert_eq!(charts.charts[1].series.len(), 3);
        assert!(!charts.charts[0].markers.is_empty());
        assert!(
            charts
                .charts
                .iter()
                .flat_map(|chart| &chart.series)
                .all(|series| series.values.len() == ga.history.points.len())
        );

        let min_conflicts = finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 3,
            max_epochs: 40,
            allow_unsolvable: true,
            ..Default::default()
        });
        let charts = metric_charts(&min_conflicts.history, palette, false).unwrap();
        assert_eq!(charts.charts.len(), 1);
        assert_eq!(charts.charts[0].x_label, "Pass");
        assert_eq!(charts.charts[0].scale, ChartScale::Conflicts);
        let labels: Vec<_> = charts.charts[0]
            .series
            .iter()
            .map(|series| series.label.as_str())
            .collect();
        assert_eq!(labels, ["Best board", "Current board"]);
    }

    #[test]
    fn comparing_different_solvers_names_both_units() {
        let ga = SavedRun {
            id: 1,
            result: finished_run(GuiConfig {
                board_size: 3,
                population_size: 1,
                ..Default::default()
            }),
        };
        let baseline = SavedRun {
            id: 2,
            result: finished_run(GuiConfig {
                solver: Solver::MinConflicts,
                board_size: 8,
                ..Default::default()
            }),
        };
        let comparison = comparison(&ga, &baseline, &Palette::LIGHT, true);
        assert_eq!(comparison.chart.x_label, "Epoch / pass");
        assert!(comparison.caption.contains("by epoch / pass"));
        assert!(comparison.context.contains("Different solvers"));
        let labels: Vec<_> = comparison
            .chart
            .series
            .iter()
            .map(|series| series.label.as_str())
            .collect();
        assert_eq!(labels, ["A · Run #1", "B · Run #2"]);
        let same = comparison_of_same(&ga);
        assert_eq!(same.chart.x_label, "Epoch");
    }

    fn comparison_of_same(run: &SavedRun) -> Comparison {
        let copy = SavedRun {
            id: run.id + 1,
            result: Arc::clone(&run.result),
        };
        comparison(run, &copy, &Palette::DARK, false)
    }
}
