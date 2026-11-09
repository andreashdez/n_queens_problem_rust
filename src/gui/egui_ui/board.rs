//! Draws the zoomable board with queen selection and conflict highlighting.

use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::{palette, point, pos, rect, square};
use crate::gui::board::{
    BoardView, EMPTY_BOARD_TEXT, HELP_TEXT, MIN_BOARD_SIDE, MIN_LABELLED_CELL, MIN_SQUARE_CELL,
    Point, QueenRole, SELECTION_INFO_HEIGHT, attacked_diagonals, attacker_outline_side,
    attacking_columns, board_summary, board_units, diagonal_band_width, diagonal_segment,
    hover_text, is_labelled_coordinate, label_spacing_and_gutter, label_stride, queen_at_pointer,
    queen_center, queen_font_size, queen_label, queen_radius, selection_text, sparse_grid_stride,
    visible_cells,
};
use crate::gui::palette::Palette;

pub(super) fn draw_empty_board(ui: &mut egui::Ui) {
    let side = ui.available_size().min_elem().max(MIN_BOARD_SIDE);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(side, side), Sense::hover());
    let palette = palette(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::ZERO, palette.empty_board);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        EMPTY_BOARD_TEXT,
        FontId::proportional(18.0),
        palette.empty_board_text.into(),
    );
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
        ui.label(board_summary(size, conflicts_sum));
        if ui
            .add_enabled(view.can_zoom_out(), egui::Button::new("−"))
            .on_hover_text("Zoom out")
            .clicked()
        {
            view.zoom_out();
        }
        if ui
            .add_enabled(view.can_zoom_in(), egui::Button::new("+"))
            .on_hover_text("Zoom in")
            .clicked()
        {
            view.zoom_in();
        }
        ui.label(view.zoom_label());
        if ui
            .button("Fit")
            .on_hover_text("Fit the whole board")
            .clicked()
        {
            *view = BoardView::default();
        }
        ui.menu_button("Help", |ui| {
            ui.set_max_width(300.0);
            for (index, group) in HELP_TEXT.iter().enumerate() {
                if index > 0 {
                    ui.separator();
                }
                for line in *group {
                    ui.label(*line);
                }
            }
        });
    });
    let (label_spacing, gutter) = label_spacing_and_gutter(size);
    // Keep room below for the selection details so selecting a queen never resizes the board.
    let board_side = (ui.available_width() - gutter)
        .min(ui.available_height() - gutter - SELECTION_INFO_HEIGHT)
        .max(MIN_BOARD_SIDE);
    let (outer, response) =
        ui.allocate_exact_size(Vec2::splat(board_side + gutter), Sense::click_and_drag());
    let viewport = Rect::from_min_size(outer.min + Vec2::splat(gutter), Vec2::splat(board_side));
    if response.dragged() {
        let delta = response.drag_delta() / board_side;
        view.move_by(Point::new(delta.x, delta.y));
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
    {
        let zoom_delta = ui.input(eframe::egui::InputState::zoom_delta);
        if (zoom_delta - 1.0).abs() > f32::EPSILON {
            let anchor = (pointer - viewport.min) / board_side;
            view.set_zoom(view.zoom() * zoom_delta, Point::new(anchor.x, anchor.y));
        }
    }
    let board = view.board(square(viewport));
    let rect = rect(board);
    let hovered = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
        .and_then(|pointer| queen_at_pointer(board, positions, point(pointer)));
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
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(hover_text(positions, conflicts, column));
    }
    let palette = palette(ui.ctx());
    let painter = ui.painter_at(viewport);
    // The last column and row labels are centred on the board edge; let them overhang it.
    let axes_painter = ui.painter_at(
        outer
            .with_max_x(outer.max.x + label_spacing / 2.0)
            .with_max_y(outer.max.y + 8.0),
    );
    let cell = rect.width() / board_units(size);
    if cell >= MIN_SQUARE_CELL {
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
        let center = queen_center(board, size, column, usize::from(positions[column]));
        let diagonals = attacked_diagonals(positions, column, &attackers);
        for (descending, attacked) in [true, false].into_iter().zip(diagonals) {
            if attacked {
                let segment = diagonal_segment(board, center, descending).map(pos);
                painter.line_segment(
                    segment,
                    Stroke::new(diagonal_band_width(cell), palette.attack_diagonal),
                );
                painter.line_segment(segment, Stroke::new(1.5, palette.attacker));
            }
        }
    }

    // Keep coordinates legible on large boards, including the selected row/column.
    let stride = label_stride(label_spacing, cell);
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
                }
                .into(),
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
                }
                .into(),
            );
        }
    }
    for (column, &raw_row) in positions.iter().enumerate() {
        let center = pos(queen_center(board, size, column, usize::from(raw_row)));
        if !viewport.expand(22.0).contains(center) {
            continue;
        }
        let attackers_of_queen = conflicts.get(column).copied().unwrap_or_default();
        let role = QueenRole::of(column, *selected, is_attacker[column], attackers_of_queen);
        let radius = queen_radius(cell);
        painter.circle_filled(center, radius, role.fill(palette));
        match role {
            QueenRole::Selected => {
                painter.circle_stroke(
                    center,
                    radius + 3.0,
                    Stroke::new(2.0, palette.selection_ring),
                );
            }
            QueenRole::Attacker => {
                // Square outlines distinguish attackers without relying on color alone.
                painter.rect_stroke(
                    Rect::from_center_size(center, Vec2::splat(attacker_outline_side(cell))),
                    egui::CornerRadius::ZERO,
                    Stroke::new(1.5, palette.attacker),
                    egui::StrokeKind::Inside,
                );
            }
            QueenRole::Dimmed | QueenRole::Normal { .. } => {}
        }
        if cell >= MIN_LABELLED_CELL
            && let Some((label, size)) =
                queen_label(attackers_of_queen, queen_font_size(cell), radius)
        {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(size),
                palette.queen_label.into(),
            );
        }
    }
    if let Some(column) = *selected {
        let (title, details) = selection_text(positions, column, &attackers);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).color(palette.selected_queen));
            if ui.small_button("Clear selection").clicked() {
                *selected = None;
            }
        });
        ui.label(details);
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
    let stride = sparse_grid_stride(cell);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoomed_large_board_layout_is_bounded_without_a_window() {
        let ctx = egui::Context::default();
        let positions: Vec<_> = (0..u16::MAX).collect();
        let conflicts = vec![u32::from(u16::MAX) - 1; positions.len()];
        let mut selected = Some(32_000);
        let mut view = BoardView::default();
        view.set_zoom(1024.0, Point::new(0.5, 0.5));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_board(ui, &positions, &conflicts, 1, &mut selected, &mut view);
        });
        assert_ne!(output.shapes.len(), 0);
        assert!(output.shapes.len() < 30_000);
        output.drop_without_applying_deltas();
    }
}
