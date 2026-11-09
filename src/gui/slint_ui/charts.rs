//! Turns the chart specs from `gui::charts` into what the Slint charts draw:
//! SVG path commands, ticks, the legend and hover text, for the current zoom and pan.

use std::{collections::HashSet, fmt::Write as _};

use super::{ChartData, LegendEntry, PlotLine, Tick, slint_color};
use crate::gui::{
    charts::{ChartSpec, X_TICK_POINTS, hover_text, linear_ticks},
    palette::Rgba,
};

/// A restart counts as hovered within this many points of the pointer.
const MARKER_HOVER_POINTS: f64 = 8.0;
/// Y-axis labels closer than this, in points, drop the minor one.
const MIN_TICK_GAP: f64 = 14.0;
/// Restart markers are dashed: this long, with gaps as long.
const DASH: f64 = 4.0;
/// The narrowest x range zooming reaches, in epochs or passes.
const MIN_SPAN: f64 = 4.0;

/// Which run's charts are on display; zoom, pan and hidden lines reset when it changes.
pub(super) type ChartKey = (u8, Option<u64>, u64);

/// Zoom, pan, hover and hidden lines, shared by the charts on display.
#[derive(Default)]
pub(super) struct ChartView {
    key: Option<ChartKey>,
    /// The x range on display, or `None` to fit all the data.
    range: Option<(f64, f64)>,
    /// The pointer, as a fraction of the plot width.
    hover: Option<f64>,
    /// Hidden legend entries, by chart title and label.
    hidden: HashSet<(String, String)>,
    /// Titles of the charts on display, by index.
    titles: Vec<String>,
    /// The plot area of each chart, in points.
    pub plot: (f32, f32),
    /// The x range that fits all the data.
    full: (f64, f64),
}

impl ChartView {
    /// Shows `charts` for `key`, keeping zoom and hidden lines while the key stays the same.
    pub(super) fn show(&mut self, key: ChartKey, charts: &[ChartSpec]) -> Vec<ChartData> {
        if self.key != Some(key) {
            self.key = Some(key);
            self.range = None;
            self.hidden.clear();
        }
        self.titles = charts.iter().map(|chart| chart.title.to_owned()).collect();
        let last = charts
            .iter()
            .flat_map(|chart| {
                let samples = chart
                    .series
                    .iter()
                    .filter_map(|series| series.values.last().map(|&(x, _)| x));
                let markers = chart.markers.iter().map(|marker| marker.epoch);
                samples.chain(markers)
            })
            .max()
            .unwrap_or(0);
        self.full = (0.0, f64::from(last).max(1.0));
        if let Some(range) = self.range {
            self.range = Some(self.clamp(range));
        }
        charts.iter().map(|chart| self.chart_data(chart)).collect()
    }

    fn range(&self) -> (f64, f64) {
        self.range.unwrap_or(self.full)
    }

    /// Keeps the range inside the data and at least [`MIN_SPAN`] wide.
    fn clamp(&self, (start, end): (f64, f64)) -> (f64, f64) {
        let (low, high) = self.full;
        let full = high - low;
        let span = (end - start).clamp(MIN_SPAN.min(full), full);
        let start = start.clamp(low, high - span);
        (start, start + span)
    }

    /// Pans by `fraction` of the plot width; dragging right shows earlier steps.
    pub(super) fn pan(&mut self, fraction: f64) {
        if !fraction.is_finite() {
            return;
        }
        let (start, end) = self.range();
        let shift = -fraction * (end - start);
        self.range = Some(self.clamp((start + shift, end + shift)));
    }

    /// Zooms by `factor` around `anchor`, a fraction of the plot width that stays put.
    pub(super) fn zoom(&mut self, factor: f64, anchor: f64) {
        if !factor.is_finite() || factor <= 0.0 || !anchor.is_finite() {
            return;
        }
        let (start, end) = self.range();
        let at = anchor.mul_add(end - start, start);
        let span = (end - start) / factor;
        let start = anchor.mul_add(-span, at);
        self.range = Some(self.clamp((start, start + span)));
    }

    pub(super) const fn reset(&mut self) {
        self.range = None;
    }

    /// The pointer's position as a fraction of the plot width, or `None` once it leaves.
    pub(super) fn hover(&mut self, fraction: Option<f64>) {
        self.hover = fraction.filter(|fraction| (0.0..=1.0).contains(fraction));
    }

    /// Hides or shows legend entry `label` of chart `index`.
    pub(super) fn toggle(&mut self, index: usize, label: &str) {
        let Some(title) = self.titles.get(index) else {
            return;
        };
        let key = (title.clone(), label.to_owned());
        if !self.hidden.remove(&key) {
            self.hidden.insert(key);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "One pass over a chart's scales, lines, ticks, legend and hover keeps them consistent."
    )]
    fn chart_data(&self, chart: &ChartSpec) -> ChartData {
        let (width, height) = (
            f64::from(self.plot.0.max(1.0)),
            f64::from(self.plot.1.max(1.0)),
        );
        let visible = |label: &str| {
            !self
                .hidden
                .contains(&(chart.title.to_owned(), label.to_owned()))
        };
        let (x0, x1) = self.range();
        let scale = chart.scale;

        // Fit the y axis to the visible lines' samples in the x range.
        let (mut y0, mut y1) = scale.included_y();
        for series in chart.series.iter().filter(|series| visible(&series.label)) {
            for &(_, value) in in_range(&series.values, x0, x1) {
                let y = scale.to_plot(value);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        // A little headroom keeps the top line clear of the frame.
        y1 = (y1 - y0).mul_add(0.05, y1);
        if y1 - y0 < 1e-9 {
            y1 = y0 + 1.0;
        }

        let to_x = |x: f64| (x - x0) / (x1 - x0) * width;
        let to_y = |y: f64| ((y - y0) / (y1 - y0)).mul_add(-height, height);

        let mut lines = Vec::new();
        for series in chart.series.iter().filter(|series| visible(&series.label)) {
            let mut commands = String::new();
            for (index, &(x, value)) in in_range(&series.values, x0, x1).iter().enumerate() {
                let command = if index == 0 { 'M' } else { 'L' };
                write!(
                    commands,
                    "{command}{:.1} {:.1} ",
                    to_x(f64::from(x)),
                    to_y(scale.to_plot(value))
                )
                .expect("writing to a String cannot fail");
            }
            lines.push(PlotLine {
                color: slint_color(series.color),
                commands: commands.into(),
            });
        }
        // Restart markers: one dashed path per color.
        let mut dashes: Vec<(Rgba, String)> = Vec::new();
        for marker in chart.markers.iter().filter(|marker| visible(&marker.label)) {
            let x = to_x(f64::from(marker.epoch));
            if !(0.0..=width).contains(&x) {
                continue;
            }
            let color = Rgba {
                a: 150,
                ..marker.color
            };
            let index = dashes
                .iter()
                .position(|(existing, _)| *existing == color)
                .unwrap_or_else(|| {
                    dashes.push((color, String::new()));
                    dashes.len() - 1
                });
            for dash in 0..dash_count(height) {
                let y = f64::from(dash) * 2.0 * DASH;
                write!(
                    dashes[index].1,
                    "M{x:.1} {y:.1} L{x:.1} {:.1} ",
                    (y + DASH).min(height)
                )
                .expect("writing to a String cannot fail");
            }
        }
        lines.extend(dashes.into_iter().map(|(color, commands)| PlotLine {
            color: slint_color(color),
            commands: commands.into(),
        }));

        let x_per_point = (x1 - x0) / width;
        let x_ticks = linear_ticks((x0, x1), x_per_point, X_TICK_POINTS)
            .into_iter()
            .map(|tick| Tick {
                pos: plot_units(to_x(tick.value)),
                text: format!("{:.0}", tick.value + 0.0).into(),
            })
            .collect::<Vec<_>>();
        let y_per_point = (y1 - y0) / height;
        let y_ticks = scale
            .ticks((y0, y1), y_per_point)
            .into_iter()
            .filter(|tick| tick.step_size / y_per_point >= MIN_TICK_GAP)
            .map(|tick| Tick {
                pos: plot_units(to_y(tick.value)),
                text: scale.format_tick(tick.value).into(),
            })
            .collect::<Vec<_>>();

        let mut legend: Vec<LegendEntry> = chart
            .series
            .iter()
            .map(|series| LegendEntry {
                label: series.label.as_str().into(),
                color: slint_color(series.color),
                hidden: !visible(&series.label),
                dashed: false,
            })
            .collect();
        for marker in &chart.markers {
            if legend
                .iter()
                .all(|entry| entry.label != marker.label.as_str())
            {
                legend.push(LegendEntry {
                    label: marker.label.as_str().into(),
                    color: slint_color(marker.color),
                    hidden: !visible(&marker.label),
                    dashed: true,
                });
            }
        }

        let (hover_x, hover) = self.hover.map_or((-1.0, None), |fraction| {
            let x = fraction.mul_add(x1 - x0, x0);
            (
                plot_units(fraction * width),
                hover_text(chart, visible, x, MARKER_HOVER_POINTS * x_per_point),
            )
        });
        ChartData {
            title: chart.title.into(),
            x_label: chart.x_label.into(),
            legend: model(legend),
            lines: model(lines),
            x_ticks: model(x_ticks),
            y_ticks: model(y_ticks),
            hover_x,
            hover_text: hover.unwrap_or_default().into(),
        }
    }
}

/// Dashes, with gaps between them, along a line `length` points long.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Plots are a few thousand points tall at most, and never negative."
)]
fn dash_count(length: f64) -> u32 {
    (length / (2.0 * DASH)).ceil().max(0.0) as u32
}

fn model<T: Clone + 'static>(items: Vec<T>) -> slint::ModelRc<T> {
    slint::ModelRc::new(slint::VecModel::from(items))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Plot positions are a few thousand points at most."
)]
const fn plot_units(value: f64) -> f32 {
    value as f32
}

/// The samples within `x0..=x1`, plus one on each side so lines reach the plot edges.
fn in_range(values: &[(u32, f32)], x0: f64, x1: f64) -> &[(u32, f32)] {
    let first = values
        .partition_point(|&(x, _)| f64::from(x) < x0)
        .saturating_sub(1);
    let end = (values.partition_point(|&(x, _)| f64::from(x) <= x1) + 1).min(values.len());
    &values[first..end.max(first)]
}

#[cfg(test)]
mod tests {
    use slint::Model as _;

    use super::*;
    use crate::gui::charts::{ChartMarker, ChartScale, ChartSeries};

    fn chart() -> ChartSpec {
        ChartSpec {
            title: "Attacking pairs",
            x_label: "Epoch",
            scale: ChartScale::Conflicts,
            series: vec![ChartSeries {
                label: "Best board".into(),
                color: Rgba::WHITE,
                values: (0..=100)
                    .map(|x| (x, 100.0 - f32::from(u8::try_from(x).unwrap())))
                    .collect(),
            }],
            markers: vec![ChartMarker {
                epoch: 50,
                label: "Soft restart".into(),
                color: Rgba::GRAY,
            }],
        }
    }

    #[test]
    fn samples_in_range_include_their_neighbours() {
        let values = [(0, 1.0), (10, 2.0), (20, 3.0), (30, 4.0)];
        assert_eq!(in_range(&values, 12.0, 18.0), &values[1..3]);
        assert_eq!(in_range(&values, 0.0, 30.0), &values[..]);
        assert_eq!(in_range(&values, 40.0, 50.0), &values[3..]);
        assert_eq!(in_range(&[], 0.0, 1.0), []);
    }

    #[test]
    fn zoom_and_pan_stay_inside_the_data() {
        let mut view = ChartView {
            plot: (500.0, 200.0),
            ..ChartView::default()
        };
        view.show((0, None, 1), &[chart()]);
        assert_eq!(view.range(), (0.0, 100.0));
        view.zoom(4.0, 0.5);
        assert_eq!(view.range(), (37.5, 62.5));
        view.pan(1.0);
        assert_eq!(view.range(), (12.5, 37.5));
        view.pan(10.0);
        assert_eq!(view.range(), (0.0, 25.0));
        view.zoom(1_000.0, 0.0);
        assert_eq!(view.range(), (0.0, MIN_SPAN));
        view.zoom(f64::NAN, 0.5);
        assert_eq!(view.range(), (0.0, MIN_SPAN));
        view.reset();
        assert_eq!(view.range(), (0.0, 100.0));
        view.zoom(2.0, 1.0);
        // Another run's charts start fitted again.
        view.show((0, None, 2), &[chart()]);
        assert_eq!(view.range(), (0.0, 100.0));
    }

    #[test]
    fn charts_list_lines_ticks_legend_and_hover() {
        let mut view = ChartView {
            plot: (500.0, 200.0),
            ..ChartView::default()
        };
        let data = view.show((0, None, 1), &[chart()]);
        let data = &data[0];
        // The series and the dashed restart marker.
        assert_eq!(data.lines.row_count(), 2);
        assert!(
            data.lines
                .row_data(0)
                .unwrap()
                .commands
                .starts_with("M0.0 ")
        );
        let legend: Vec<_> = data.legend.iter().map(|entry| entry.label).collect();
        assert_eq!(legend, ["Best board", "Soft restart"]);
        assert!(data.x_ticks.row_count() >= 3);
        assert_eq!(data.y_ticks.row_data(0).unwrap().text, "0");
        assert!(data.hover_x < 0.0);

        view.hover(Some(0.5));
        view.toggle(0, "Soft restart");
        let data = view.show((0, None, 1), &[chart()]);
        assert_eq!(data[0].hover_x, 250.0);
        assert_eq!(data[0].hover_text, "Best board · epoch 50: 50");
        assert_eq!(data[0].lines.row_count(), 1);
        assert!(data[0].legend.row_data(1).unwrap().hidden);
        view.hover(None);
        assert!(view.show((0, None, 1), &[chart()])[0].hover_x < 0.0);
    }
}
