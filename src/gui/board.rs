//! The zoomable board with queen selection and conflict highlighting.

use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::{attacking_pairs, palette::Palette};

const MIN_BOARD_SIDE: f32 = 160.0;
const SELECTION_INFO_HEIGHT: f32 = 44.0;

pub(super) fn draw_empty_board(ui: &mut egui::Ui) {
    let side = ui.available_size().min_elem().max(MIN_BOARD_SIDE);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(side, side), Sense::hover());
    let palette = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::ZERO, palette.empty_board);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Run the solver to draw a board",
        FontId::proportional(18.0),
        palette.empty_board_text,
    );
}

/// Pan is measured in viewport widths, so resizing preserves the viewed board region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoardView {
    zoom: f32,
    pan: Vec2,
}

impl Default for BoardView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl BoardView {
    pub(super) fn set_zoom(&mut self, zoom: f32, anchor: Vec2) {
        if !zoom.is_finite() || !anchor.x.is_finite() || !anchor.y.is_finite() {
            return;
        }
        let next = zoom.clamp(1.0, 1024.0);
        self.pan = anchor - (anchor - self.pan) * (next / self.zoom);
        self.zoom = next;
        self.clamp_pan();
    }

    fn move_by(&mut self, delta: Vec2) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        self.pan += delta;
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        self.pan = self.pan.clamp(Vec2::splat(1.0 - self.zoom), Vec2::ZERO);
    }

    fn rect(self, viewport: Rect) -> Rect {
        Rect::from_min_size(
            viewport.min + self.pan * viewport.width(),
            viewport.size() * self.zoom,
        )
    }
}

// A board dimension and its coordinate indices fit the solver's u16 range.
fn board_units(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("board coordinate fits u16"))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Screen-space indices intentionally round down; conversion saturates and callers bound indices to the board."
)]
const fn cell_index(value: f32) -> usize {
    value.max(0.0) as usize
}

fn visible_cells(start: f32, cell: f32, min: f32, max: f32, size: usize) -> std::ops::Range<usize> {
    let first = cell_index(((min - start) / cell).floor()).min(size);
    let end = cell_index(((max - start) / cell).ceil()).min(size);
    first..end.max(first)
}

/// Positions are a permutation, so only diagonals can contain attacking queens.
fn attacking_columns(positions: &[u16], selected: usize) -> Vec<usize> {
    let Some(&row) = positions.get(selected) else {
        return Vec::new();
    };
    positions
        .iter()
        .enumerate()
        .filter(|&(column, &other_row)| {
            column != selected && column.abs_diff(selected) == usize::from(other_row.abs_diff(row))
        })
        .map(|(column, _)| column)
        .collect()
}

fn queen_at_pointer(rect: Rect, positions: &[u16], pointer: Pos2) -> Option<usize> {
    if positions.is_empty()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
        || pointer.x < rect.left()
        || pointer.x >= rect.right()
        || pointer.y < rect.top()
        || pointer.y >= rect.bottom()
    {
        return None;
    }
    let column =
        cell_index((pointer.x - rect.left()) / rect.width() * board_units(positions.len()));
    let row = cell_index((pointer.y - rect.top()) / rect.height() * board_units(positions.len()));
    positions
        .get(column)
        .filter(|&&queen_row| usize::from(queen_row) == row)
        .map(|_| column)
}

fn queen_center(rect: Rect, size: usize, column: usize, row: usize) -> Pos2 {
    let cell = rect.width() / board_units(size);
    Pos2::new(
        (board_units(column) + 0.5).mul_add(cell, rect.left()),
        (board_units(row) + 0.5).mul_add(cell, rect.top()),
    )
}

/// Endpoints of a full diagonal, clipped to the board's outer edges.
fn diagonal_segment(rect: Rect, center: Pos2, descending: bool) -> [Pos2; 2] {
    if descending {
        let before = (center.x - rect.left()).min(center.y - rect.top());
        let after = (rect.right() - center.x).min(rect.bottom() - center.y);
        [center - Vec2::splat(before), center + Vec2::splat(after)]
    } else {
        let before = (center.x - rect.left()).min(rect.bottom() - center.y);
        let after = (rect.right() - center.x).min(center.y - rect.top());
        [
            center + Vec2::new(-before, before),
            center + Vec2::new(after, -after),
        ]
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Keep immediate-mode board interaction and its layered rendering in the same frame scope."
)]
pub(super) fn draw_board(
    ui: &mut egui::Ui,
    positions: &[u16],
    conflicts: &[u32],
    conflicts_sum: u32,
    selected: &mut Option<usize>,
    view: &mut BoardView,
) {
    let size = positions.len();
    if size == 0 {
        *selected = None;
        draw_empty_board(ui);
        return;
    }
    if selected.is_some_and(|column| column >= size) {
        *selected = None;
    }

    ui.horizontal_wrapped(|ui| {
        ui.heading("Board");
        ui.label(format!("{size} queens · {}", attacking_pairs(conflicts_sum)));
        if ui
            .add_enabled(view.zoom > 1.0, egui::Button::new("−"))
            .on_hover_text("Zoom out")
            .clicked()
        {
            view.set_zoom(view.zoom / 1.5, Vec2::splat(0.5));
        }
        if ui
            .add_enabled(view.zoom < 1024.0, egui::Button::new("+"))
            .on_hover_text("Zoom in")
            .clicked()
        {
            view.set_zoom(view.zoom * 1.5, Vec2::splat(0.5));
        }
        ui.label(format!("{:.0}%", view.zoom * 100.0));
        if ui.button("Fit").on_hover_text("Fit the whole board").clicked() {
            *view = BoardView::default();
        }
        ui.menu_button("Help", |ui| {
            ui.set_max_width(300.0);
            ui.label("A number on a queen counts the queens attacking it; Q marks a queen nobody attacks.");
            ui.separator();
            ui.label("Click a queen to inspect its conflicts. Click it again or an empty square to clear.");
            ui.label("Press Escape while the board is focused to clear the selection. Otherwise Escape cancels a running solver.");
            ui.separator();
            ui.label("Drag to pan · Ctrl/Cmd + scroll or pinch to zoom.");
            ui.label("Fit resets the zoom and pan to show the whole board.");
            ui.separator();
            ui.label("Coordinates are 1-based: columns left to right, rows top to bottom.");
        });
    });
    // Labels sit at least one label width apart, so long numbers do not collide, and the
    // gutter fits the widest row label to the left of the board.
    let digits = board_units(size.to_string().len());
    let label_spacing = 7.0f32.mul_add(digits, 6.0).max(24.0);
    let gutter = 7.0f32.mul_add(digits, 8.0).max(28.0);
    // Keep room below for the selection details so selecting a queen never resizes the board.
    let board_side = (ui.available_width() - gutter)
        .min(ui.available_height() - gutter - SELECTION_INFO_HEIGHT)
        .max(MIN_BOARD_SIDE);
    let (outer, response) =
        ui.allocate_exact_size(Vec2::splat(board_side + gutter), Sense::click_and_drag());
    let viewport = Rect::from_min_size(outer.min + Vec2::splat(gutter), Vec2::splat(board_side));
    if response.dragged() {
        view.move_by(response.drag_delta() / board_side);
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
    {
        let zoom_delta = ui.input(eframe::egui::InputState::zoom_delta);
        if (zoom_delta - 1.0).abs() > f32::EPSILON {
            view.set_zoom(
                view.zoom * zoom_delta,
                (pointer - viewport.min) / board_side,
            );
        }
    }
    let rect = view.rect(viewport);
    let hovered = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
        .and_then(|pointer| queen_at_pointer(rect, positions, pointer));
    if response.clicked() {
        *selected = if hovered == *selected { None } else { hovered };
    }
    // Consumed so the app-wide Escape shortcut (cancel run) does not also fire.
    if response.has_focus()
        && selected.is_some()
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        *selected = None;
    }
    if let Some(column) = hovered {
        let count = conflicts.get(column).copied().unwrap_or_default();
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "Column {}, row {}\n{} attacking queen{}\nClick to inspect",
                column + 1,
                usize::from(positions[column]) + 1,
                count,
                if count == 1 { "" } else { "s" }
            ));
    }
    let palette = Palette::of(ui.ctx());
    let painter = ui.painter_at(viewport);
    // The last column and row labels are centred on the board edge; let them overhang it.
    let axes_painter = ui.painter_at(
        outer
            .with_max_x(outer.max.x + label_spacing / 2.0)
            .with_max_y(outer.max.y + 8.0),
    );
    let cell = rect.width() / board_units(size);
    if cell >= 6.0 {
        draw_board_cells(&painter, palette, rect, size, cell);
    } else {
        draw_sparse_grid(&painter, palette, rect, size, cell);
    }

    let attackers = selected
        .map(|column| attacking_columns(positions, column))
        .unwrap_or_default();
    let mut is_attacker = vec![false; size];
    for &column in &attackers {
        is_attacker[column] = true;
    }
    if let Some(column) = *selected {
        let row = usize::from(positions[column]);
        let center = queen_center(rect, size, column, row);
        for descending in [true, false] {
            if attackers.iter().any(|&other| {
                ((other > column) == (usize::from(positions[other]) > row)) == descending
            }) {
                let segment = diagonal_segment(rect, center, descending);
                painter.line_segment(
                    segment,
                    Stroke::new((cell * 0.24).clamp(3.0, 10.0), palette.attack_diagonal),
                );
                painter.line_segment(segment, Stroke::new(1.5, palette.attacker));
            }
        }
    }

    // Keep coordinates legible on large boards, including the selected row/column.
    let stride = cell_index((label_spacing / cell).ceil()).max(1);
    for index in 0..size {
        let regular = is_labelled_coordinate(index, size, stride);
        let selected_column = *selected == Some(index);
        let selected_row = selected.is_some_and(|column| usize::from(positions[column]) == index);
        let coordinate = (index + 1).to_string();
        let font = FontId::monospace(11.0);
        let x = (board_units(index) + 0.5).mul_add(cell, rect.left());
        let y = (board_units(index) + 0.5).mul_add(cell, rect.top());
        if (regular || selected_column) && x >= viewport.left() && x <= viewport.right() {
            axes_painter.text(
                Pos2::new(x, viewport.top() - 12.0),
                Align2::CENTER_CENTER,
                &coordinate,
                font.clone(),
                if selected_column {
                    palette.selected_queen
                } else {
                    palette.coordinate
                },
            );
        }
        if (regular || selected_row) && y >= viewport.top() && y <= viewport.bottom() {
            axes_painter.text(
                Pos2::new(viewport.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                &coordinate,
                font,
                if selected_row {
                    palette.selected_queen
                } else {
                    palette.coordinate
                },
            );
        }
    }
    for (column, &raw_row) in positions.iter().enumerate() {
        let center = queen_center(rect, size, column, usize::from(raw_row));
        if !viewport.expand(22.0).contains(center) {
            continue;
        }
        let is_selected = *selected == Some(column);
        let color = if is_selected {
            palette.selected_queen
        } else if is_attacker[column] {
            palette.attacker
        } else if selected.is_some() {
            palette.queen_dimmed
        } else {
            palette.queen(conflicts.get(column).copied().unwrap_or_default())
        };
        let radius = (cell * 0.34).clamp(2.0, 18.0);
        painter.circle_filled(center, radius, color);
        if is_selected {
            painter.circle_stroke(
                center,
                radius + 3.0,
                Stroke::new(2.0, palette.selection_ring),
            );
        } else if is_attacker[column] {
            // Square outlines distinguish attackers without relying on color alone.
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat((cell * 0.88).max(5.0))),
                egui::CornerRadius::ZERO,
                Stroke::new(1.5, palette.attacker),
                egui::StrokeKind::Inside,
            );
        }
        if cell >= 16.0
            && let Some((label, size)) = queen_label(
                conflicts.get(column).copied().unwrap_or_default(),
                (cell * 0.42).clamp(10.0, 24.0),
                radius,
            )
        {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(size),
                palette.queen_label,
            );
        }
    }
    if let Some(column) = *selected {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Selected: column {}, row {}",
                    column + 1,
                    usize::from(positions[column]) + 1
                ))
                .color(palette.selected_queen),
            );
            if ui.small_button("Clear selection").clicked() {
                *selected = None;
            }
        });
        if attackers.is_empty() {
            ui.label("This queen has no conflicts.");
        } else {
            let coordinates = attackers
                .iter()
                .take(8)
                .map(|&other| format!("C{} R{}", other + 1, usize::from(positions[other]) + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let more = if attackers.len() > 8 {
                format!(" and {} more", attackers.len() - 8)
            } else {
                String::new()
            };
            ui.label(format!("Attacked by {} queen{}: {coordinates}{more}. Highlighted lines show their shared diagonals.", attackers.len(), if attackers.len() == 1 { "" } else { "s" }));
        }
    }
}

fn draw_board_cells(
    painter: &egui::Painter,
    palette: &Palette,
    rect: Rect,
    size: usize,
    cell: f32,
) {
    let dark = palette.dark_square;
    let light = palette.light_square;
    let clip = painter.clip_rect();
    for y in visible_cells(rect.top(), cell, clip.top(), clip.bottom(), size) {
        for x in visible_cells(rect.left(), cell, clip.left(), clip.right(), size) {
            let cell_rect = Rect::from_min_max(
                Pos2::new(
                    board_units(x).mul_add(cell, rect.left()),
                    board_units(y).mul_add(cell, rect.top()),
                ),
                Pos2::new(
                    board_units(x + 1).mul_add(cell, rect.left()),
                    board_units(y + 1).mul_add(cell, rect.top()),
                ),
            );
            let color = if (x + y) % 2 == 0 { light } else { dark };
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, color);
        }
    }
}

fn draw_sparse_grid(
    painter: &egui::Painter,
    palette: &Palette,
    rect: Rect,
    size: usize,
    cell: f32,
) {
    painter.rect_filled(rect, egui::CornerRadius::ZERO, palette.sparse_board);
    let stroke = Stroke::new(1.0, palette.sparse_grid_line);
    let clip = painter.clip_rect();
    let stride = cell_index((24.0 / cell).ceil()).max(1);
    for index in visible_cells(rect.left(), cell, clip.left(), clip.right(), size + 1)
        .filter(|index| index.is_multiple_of(stride))
    {
        let x = board_units(index).mul_add(cell, rect.left());
        painter.line_segment(
            [Pos2::new(x, clip.top()), Pos2::new(x, clip.bottom())],
            stroke,
        );
    }
    for index in visible_cells(rect.top(), cell, clip.top(), clip.bottom(), size + 1)
        .filter(|index| index.is_multiple_of(stride))
    {
        let y = board_units(index).mul_add(cell, rect.top());
        painter.line_segment(
            [Pos2::new(clip.left(), y), Pos2::new(clip.right(), y)],
            stroke,
        );
    }
}

/// The attacker count, so conflicts read without relying on color; "Q" when there are none.
/// Every `stride`-th coordinate is labelled, plus the last one; a sampled label
/// closer than `stride` to the last is dropped so the two never overlap.
const fn is_labelled_coordinate(index: usize, size: usize, stride: usize) -> bool {
    index + 1 == size || (index.is_multiple_of(stride) && size - 1 - index >= stride)
}

/// Longer counts get a smaller font, and labels that still overflow the queen are skipped.
fn queen_label(attackers: u32, font_size: f32, radius: f32) -> Option<(String, f32)> {
    let label = if attackers == 0 {
        "Q".to_owned()
    } else {
        attackers.to_string()
    };
    let size = font_size
        * match label.len() {
            1 | 2 => 1.0,
            3 => 0.75,
            _ => 0.55,
        };
    // Digits are about 0.6 em wide in the proportional font.
    let digits = f32::from(u8::try_from(label.len()).unwrap_or(u8::MAX));
    (0.6 * size * digits <= 2.0 * radius).then_some((label, size))
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "These tests check clamped zoom limits, which are exact."
)]
mod tests {
    use super::*;
    use crate::ga;

    #[test]
    fn board_zoom_preserves_pointer_anchor_and_pan_is_bounded() {
        let viewport = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(400.0));
        let positions = [1, 3, 0, 2];
        let mut view = BoardView::default();
        let point = queen_center(viewport, 4, 2, 0);
        let anchor = (point - viewport.min) / viewport.width();
        view.set_zoom(4.0, anchor);
        assert_eq!(
            queen_at_pointer(view.rect(viewport), &positions, point),
            Some(2)
        );
        view.move_by(Vec2::splat(100.0));
        assert_eq!(view.pan, Vec2::ZERO);
        view.move_by(Vec2::splat(-100.0));
        assert_eq!(view.pan, Vec2::splat(-3.0));
        let before = view;
        view.set_zoom(f32::NAN, anchor);
        assert_eq!(view, before);
        view.set_zoom(1.0, anchor);
        assert_eq!(view, BoardView::default());
        view.set_zoom(100_000.0, anchor);
        assert_eq!(view.zoom, 1024.0);
    }

    #[test]
    fn visible_cell_range_limits_work_on_large_zoomed_boards() {
        let range = visible_cells(-9_900.0, 10.0, 0.0, 400.0, 65_535);
        assert_eq!(range, 990..1030);
        assert_eq!(visible_cells(500.0, 10.0, 0.0, 400.0, 100), 0..0);
        assert_eq!(visible_cells(0.0, 10.0, 0.0, 1000.0, 4), 0..4);
    }

    #[test]
    fn zoomed_large_board_layout_is_bounded_without_a_window() {
        let ctx = egui::Context::default();
        let positions: Vec<_> = (0..u16::MAX).collect();
        let conflicts = vec![u32::from(u16::MAX) - 1; positions.len()];
        let mut selected = Some(32_000);
        let mut view = BoardView::default();
        view.set_zoom(1024.0, Vec2::splat(0.5));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_board(ui, &positions, &conflicts, 1, &mut selected, &mut view);
        });
        assert_ne!(output.shapes.len(), 0);
        assert!(output.shapes.len() < 30_000);
        output.drop_without_applying_deltas();
    }

    #[test]
    fn selected_queen_attackers_match_solver_counts() {
        for positions in [
            vec![0, 1, 2, 3, 4],
            vec![4, 3, 2, 1, 0],
            vec![0, 2, 1, 3, 4],
            vec![1, 3, 0, 2],
            vec![0],
        ] {
            let chromosome = ga::chromosome::Chromosome::new(positions.clone());
            for column in 0..positions.len() {
                let attackers = attacking_columns(&positions, column);
                assert_eq!(attackers.len(), chromosome.get_conflicts()[column] as usize);
                assert!(!attackers.contains(&column));
                assert!(
                    attackers
                        .iter()
                        .all(|&other| attacking_columns(&positions, other).contains(&column))
                );
            }
        }
        assert_eq!(attacking_columns(&[0, 2, 1, 3, 4], 1), vec![2]);
        assert_eq!(attacking_columns(&[1, 3, 0, 2], 0), Vec::<usize>::new());
        assert_eq!(attacking_columns(&[], 0), Vec::<usize>::new());
        assert_eq!(attacking_columns(&[0], 5), Vec::<usize>::new());
    }

    #[test]
    fn board_hit_testing_distinguishes_queens_empty_squares_and_edges() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(80.0));
        let positions = [1, 3, 0, 2];
        for (column, &row) in positions.iter().enumerate() {
            assert_eq!(
                queen_at_pointer(
                    rect,
                    &positions,
                    queen_center(rect, 4, column, row as usize)
                ),
                Some(column)
            );
        }
        for point in [
            Pos2::new(10.0, 20.0),
            Pos2::new(9.0, 50.0),
            Pos2::new(90.0, 50.0),
            Pos2::new(50.0, 100.0),
        ] {
            assert_eq!(queen_at_pointer(rect, &positions, point), None);
        }
        assert_eq!(queen_at_pointer(rect, &[], rect.center()), None);
        assert_eq!(queen_at_pointer(rect, &[0], rect.min), Some(0));
    }

    #[test]
    fn queens_show_attacker_counts_and_shrink_long_ones() {
        let labelled = |size, stride| {
            (0..size)
                .filter(|&index| is_labelled_coordinate(index, size, stride))
                .map(|index| index + 1)
                .collect::<Vec<_>>()
        };
        assert_eq!(labelled(4, 1), [1, 2, 3, 4]);
        assert_eq!(labelled(18, 2), [1, 3, 5, 7, 9, 11, 13, 15, 18]);
        assert_eq!(labelled(1_000, 70).last_chunk(), Some(&[911, 1_000]));
        assert_eq!(labelled(1, 5), [1]);
        let label = |attackers| queen_label(attackers, 20.0, 18.0);
        assert_eq!(label(0), Some(("Q".to_owned(), 20.0)));
        assert_eq!(label(3), Some(("3".to_owned(), 20.0)));
        assert_eq!(label(42), Some(("42".to_owned(), 20.0)));
        assert_eq!(label(999), Some(("999".to_owned(), 15.0)));
        assert_eq!(label(65_534), Some(("65534".to_owned(), 11.0)));
        // Small queens keep single digits but drop counts that would spill out.
        assert!(queen_label(7, 10.0, 5.5).is_some());
        assert!(queen_label(12, 10.0, 5.5).is_none());
    }

    #[test]
    fn highlighted_diagonals_reach_board_edges() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(80.0));
        assert_eq!(
            diagonal_segment(rect, rect.center(), true),
            [rect.left_top(), rect.right_bottom()]
        );
        assert_eq!(
            diagonal_segment(rect, rect.center(), false),
            [rect.left_bottom(), rect.right_top()]
        );
        let center = Pos2::new(20.0, 50.0);
        assert_eq!(
            diagonal_segment(rect, center, true),
            [Pos2::new(10.0, 40.0), Pos2::new(70.0, 100.0)]
        );
        assert_eq!(
            diagonal_segment(rect, center, false),
            [Pos2::new(10.0, 60.0), Pos2::new(50.0, 20.0)]
        );
    }
}
