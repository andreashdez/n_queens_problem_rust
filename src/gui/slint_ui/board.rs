//! Draws the board into an image with tiny-skia, at the window's physical
//! resolution, and lays out the text Slint draws over and around it.
//!
//! Slint's own elements would need one item per square and queen; a board can
//! have 65,535 queens, so it is drawn as one image instead.

use slint::{Rgba8Pixel, SharedPixelBuffer};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform};

use super::{BoardText, slint_color};
use crate::gui::{
    board::{
        BoardView, MIN_BOARD_SIDE, MIN_LABELLED_CELL, MIN_SQUARE_CELL, Point, QueenRole, Square,
        attacked_diagonals, attacker_outline_side, attacking_columns, board_units, cell_index,
        diagonal_band_width, diagonal_segment, is_labelled_coordinate, label_spacing_and_gutter,
        label_stride, queen_center, queen_font_size, queen_label, queen_radius, sparse_grid_stride,
        visible_cells,
    },
    palette::{Palette, Rgba},
};

/// Queens with a smaller radius, in physical pixels, are drawn as squares of
/// pixels: tens of thousands of tiny anti-aliased circles cost tens of milliseconds.
const MIN_ROUND_QUEEN: f32 = 3.0;
/// Coordinate labels' font size, in points.
const COORDINATE_FONT: f32 = 11.0;

/// The board on display and how it is viewed.
pub(super) struct Scene<'a> {
    pub positions: &'a [u16],
    pub conflicts: &'a [u32],
    pub selected: Option<usize>,
    pub view: BoardView,
    pub palette: &'static Palette,
}

impl Scene<'_> {
    const fn size(&self) -> usize {
        self.positions.len()
    }
}

/// The square viewport for a board of `size` queens, or an empty board when
/// `size` is `None`, in a board area `width` by `height` points. A board leaves
/// a gutter on the top and left for coordinates.
pub(super) fn viewport(queens: Option<usize>, width: f32, height: f32) -> Square {
    let Some(queens) = queens else {
        let extent = width.min(height).max(MIN_BOARD_SIDE);
        return Square {
            left: ((width - extent) / 2.0).max(0.0),
            top: 0.0,
            side: extent,
        };
    };
    let (_, gutter) = label_spacing_and_gutter(queens);
    let extent = (width - gutter).min(height - gutter).max(MIN_BOARD_SIDE);
    Square {
        left: gutter + ((width - gutter - extent) / 2.0).max(0.0),
        top: gutter,
        side: extent,
    }
}

fn skia(color: Rgba) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.r, color.g, color.b, color.a)
}

fn paint(color: Rgba, anti_alias: bool) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias,
        ..Paint::default()
    };
    paint.set_color(skia(color));
    paint
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Pixel sizes are small, positive and rounded first."
)]
const fn pixels(value: f32) -> u32 {
    value as u32
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Pixel sizes are far below f32's exact integer range."
)]
const fn pixel_units(value: u32) -> f32 {
    value as f32
}

/// The board drawn into a square `points` wide, at `scale` physical pixels per point.
pub(super) fn render(scene: &Scene<'_>, points: f32, scale: f32) -> SharedPixelBuffer<Rgba8Pixel> {
    let pixels_wide = pixels((points * scale).round().max(1.0));
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(pixels_wide, pixels_wide);
    let Some(mut pixmap) = PixmapMut::from_bytes(buffer.make_mut_bytes(), pixels_wide, pixels_wide)
    else {
        return buffer;
    };
    let palette = scene.palette;
    pixmap.fill(skia(palette.empty_board));
    let size = scene.size();
    if size == 0 {
        return buffer;
    }

    let extent = pixel_units(pixels_wide);
    let board = scene.view.board(Square {
        left: 0.0,
        top: 0.0,
        side: extent,
    });
    let cell = board.side / board_units(size);
    // Sizes from `board.rs` are in points.
    let cell_points = cell / scale;
    if cell_points >= MIN_SQUARE_CELL {
        draw_squares(&mut pixmap, palette, board, size, cell);
    } else {
        draw_sparse_grid(&mut pixmap, palette, board, size, cell, scale);
    }

    let attackers = scene
        .selected
        .map(|column| attacking_columns(scene.positions, column))
        .unwrap_or_default();
    if let Some(column) = scene.selected {
        draw_diagonals(&mut pixmap, scene, board, column, &attackers, scale);
    }
    draw_queens(&mut pixmap, scene, board, &attackers, scale);
    buffer
}

fn draw_squares(
    pixmap: &mut PixmapMut<'_>,
    palette: &Palette,
    board: Square,
    size: usize,
    cell: f32,
) {
    let extent = pixel_units(pixmap.width());
    let columns = visible_cells(board.left, cell, 0.0, extent, size);
    let rows = visible_cells(board.top, cell, 0.0, extent, size);
    // Edges snap to whole pixels so neighbouring squares never leave seams.
    let edge = |start: f32, index: usize| board_units(index).mul_add(cell, start).round();
    if let Some(rect) = Rect::from_ltrb(
        edge(board.left, columns.start).max(0.0),
        edge(board.top, rows.start).max(0.0),
        edge(board.left, columns.end).min(extent),
        edge(board.top, rows.end).min(extent),
    ) {
        pixmap.fill_rect(
            rect,
            &paint(palette.light_square, false),
            Transform::identity(),
            None,
        );
    }
    let dark = paint(palette.dark_square, false);
    for row in rows {
        let (top, bottom) = (edge(board.top, row), edge(board.top, row + 1));
        // Squares whose coordinates add up to an odd number are dark.
        let mut column = columns.start + (row + columns.start + 1) % 2;
        while column < columns.end {
            if let Some(rect) = Rect::from_ltrb(
                edge(board.left, column),
                top,
                edge(board.left, column + 1),
                bottom,
            ) {
                pixmap.fill_rect(rect, &dark, Transform::identity(), None);
            }
            column += 2;
        }
    }
}

/// Too dense for squares: a plain board with every few grid lines.
fn draw_sparse_grid(
    pixmap: &mut PixmapMut<'_>,
    palette: &Palette,
    board: Square,
    size: usize,
    cell: f32,
    scale: f32,
) {
    let extent = pixel_units(pixmap.width());
    pixmap.fill(skia(palette.sparse_board));
    let line = paint(palette.sparse_grid_line, false);
    let width = scale.round().max(1.0);
    let stride = sparse_grid_stride(cell / scale);
    let lines = |start: f32| {
        visible_cells(start, cell, 0.0, extent, size + 1)
            .filter(move |index| index.is_multiple_of(stride))
            .map(move |index| board_units(index).mul_add(cell, start).round())
    };
    for x in lines(board.left) {
        if let Some(rect) = Rect::from_xywh(x, 0.0, width, extent) {
            pixmap.fill_rect(rect, &line, Transform::identity(), None);
        }
    }
    for y in lines(board.top) {
        if let Some(rect) = Rect::from_xywh(0.0, y, extent, width) {
            pixmap.fill_rect(rect, &line, Transform::identity(), None);
        }
    }
}

/// The selected queen's diagonals that hold an attacker, as a wide band under a thin line.
fn draw_diagonals(
    pixmap: &mut PixmapMut<'_>,
    scene: &Scene<'_>,
    board: Square,
    column: usize,
    attackers: &[usize],
    scale: f32,
) {
    let size = scene.size();
    let cell_points = board.side / board_units(size) / scale;
    let center = queen_center(board, size, column, usize::from(scene.positions[column]));
    let mut path = PathBuilder::new();
    for (descending, attacked) in
        [true, false]
            .into_iter()
            .zip(attacked_diagonals(scene.positions, column, attackers))
    {
        if attacked {
            let [start, end] = diagonal_segment(board, center, descending);
            path.move_to(start.x, start.y);
            path.line_to(end.x, end.y);
        }
    }
    let Some(path) = path.finish() else {
        return;
    };
    let palette = scene.palette;
    for (color, width) in [
        (palette.attack_diagonal, diagonal_band_width(cell_points)),
        (palette.attacker, 1.5),
    ] {
        let stroke = Stroke {
            width: width * scale,
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint(color, true),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn draw_queens(
    pixmap: &mut PixmapMut<'_>,
    scene: &Scene<'_>,
    board: Square,
    attackers: &[usize],
    scale: f32,
) {
    let size = scene.size();
    let extent = pixel_units(pixmap.width());
    let cell = board.side / board_units(size);
    let cell_points = cell / scale;
    let radius = queen_radius(cell_points) * scale;
    // Room for the selection ring around queens just outside the viewport.
    let margin = 4.0f32.mul_add(scale, radius);
    let mut is_attacker = vec![false; size];
    for &column in attackers {
        is_attacker[column] = true;
    }

    // Selected and attacking queens go on top of the rest.
    let mut layers: [Vec<(Point, Rgba)>; 3] = Default::default();
    for column in visible_cells(board.left, cell, -margin, extent + margin, size) {
        let row = usize::from(scene.positions[column]);
        let center = queen_center(board, size, column, row);
        if center.y < -margin || center.y > extent + margin {
            continue;
        }
        let conflicts = scene.conflicts.get(column).copied().unwrap_or_default();
        let role = QueenRole::of(column, scene.selected, is_attacker[column], conflicts);
        let layer = match role {
            QueenRole::Selected => 2,
            QueenRole::Attacker => 1,
            QueenRole::Dimmed | QueenRole::Normal { .. } => 0,
        };
        layers[layer].push((center, role.fill(scene.palette)));
    }

    for queens in &layers {
        if radius < MIN_ROUND_QUEEN {
            fill_pixel_squares(pixmap, queens, radius);
        } else {
            fill_circles(pixmap, queens, radius);
        }
    }

    let palette = scene.palette;
    let outline = |pixmap: &mut PixmapMut<'_>, path: Option<tiny_skia::Path>, color, width: f32| {
        if let Some(path) = path {
            let stroke = Stroke {
                width: width * scale,
                ..Stroke::default()
            };
            pixmap.stroke_path(
                &path,
                &paint(color, true),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    };
    // Square outlines mark attackers without relying on color alone.
    let outline_side = attacker_outline_side(cell_points) * scale;
    // The stroke stays inside the outline, as egui drew it.
    let inset = 0.75 * scale;
    let inner_side = 2.0f32.mul_add(-inset, outline_side);
    let mut squares = PathBuilder::new();
    for (center, _) in &layers[1] {
        if let Some(rect) = Rect::from_xywh(
            center.x - outline_side / 2.0 + inset,
            center.y - outline_side / 2.0 + inset,
            inner_side,
            inner_side,
        ) {
            squares.push_rect(rect);
        }
    }
    outline(pixmap, squares.finish(), palette.attacker, 1.5);
    let mut rings = PathBuilder::new();
    for (center, _) in &layers[2] {
        rings.push_circle(center.x, center.y, 3.0f32.mul_add(scale, radius));
    }
    outline(pixmap, rings.finish(), palette.selection_ring, 2.0);
}

/// Anti-aliased circles, one path per color.
fn fill_circles(pixmap: &mut PixmapMut<'_>, queens: &[(Point, Rgba)], radius: f32) {
    let mut by_color: Vec<(Rgba, PathBuilder)> = Vec::new();
    for &(center, color) in queens {
        let index = by_color
            .iter()
            .position(|(existing, _)| *existing == color)
            .unwrap_or_else(|| {
                by_color.push((color, PathBuilder::new()));
                by_color.len() - 1
            });
        by_color[index].1.push_circle(center.x, center.y, radius);
    }
    for (color, builder) in by_color {
        if let Some(path) = builder.finish() {
            pixmap.fill_path(
                &path,
                &paint(color, true),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// Opaque squares written straight into the pixels, for queens too small to look round.
fn fill_pixel_squares(pixmap: &mut PixmapMut<'_>, queens: &[(Point, Rgba)], radius: f32) {
    let extent = pixmap.width() as usize;
    let width = (2.0 * radius).round().max(1.0);
    let data = pixmap.data_mut();
    for &(center, color) in queens {
        let start = |middle: f32| cell_index((middle - width / 2.0).round()).min(extent);
        let (left, top) = (start(center.x), start(center.y));
        let (right, bottom) = (
            (left + cell_index(width)).min(extent),
            (top + cell_index(width)).min(extent),
        );
        // Queen fills are opaque, so straight and premultiplied alpha agree.
        let pixel = [color.r, color.g, color.b, 255];
        for row in top..bottom {
            let (targets, _) =
                data[(row * extent + left) * 4..(row * extent + right) * 4].as_chunks_mut::<4>();
            targets.fill(pixel);
        }
    }
}

/// Attacker counts on queens big enough to hold them, relative to the viewport.
pub(super) fn queen_labels(scene: &Scene<'_>, viewport: Square) -> Vec<BoardText> {
    let size = scene.size();
    let local = Square {
        left: 0.0,
        top: 0.0,
        side: viewport.side,
    };
    let board = scene.view.board(local);
    let cell = board.side / board_units(size);
    if size == 0 || cell < MIN_LABELLED_CELL {
        return Vec::new();
    }
    let radius = queen_radius(cell);
    let font_size = queen_font_size(cell);
    visible_cells(board.left, cell, -radius, local.side + radius, size)
        .filter_map(|column| {
            let center = queen_center(board, size, column, usize::from(scene.positions[column]));
            if center.y < -radius || center.y > local.side + radius {
                return None;
            }
            let conflicts = scene.conflicts.get(column).copied().unwrap_or_default();
            let (text, size) = queen_label(conflicts, font_size, radius)?;
            Some(BoardText {
                x: center.x,
                y: center.y,
                width: 2.0f32.mul_add(radius, 8.0),
                text: text.into(),
                color: slint_color(scene.palette.queen_label),
                size,
                right: false,
            })
        })
        .collect()
}

/// Column numbers above the board and row numbers to its left, in board-area
/// coordinates. The selected queen's column and row are always labelled.
pub(super) fn coordinate_labels(scene: &Scene<'_>, viewport: Square) -> Vec<BoardText> {
    let size = scene.size();
    if size == 0 {
        return Vec::new();
    }
    let board = scene.view.board(viewport);
    let cell = board.side / board_units(size);
    let (label_spacing, _) = label_spacing_and_gutter(size);
    let stride = label_stride(label_spacing, cell);
    let selected_row = scene
        .selected
        .map(|column| usize::from(scene.positions[column]));
    let palette = scene.palette;
    let color = |selected: bool| {
        slint_color(if selected {
            palette.selected_queen
        } else {
            palette.coordinate
        })
    };
    let half_height = 7.0;
    let mut labels = Vec::new();
    for index in visible_cells(board.left, cell, viewport.left, viewport.right(), size) {
        let selected = scene.selected == Some(index);
        let x = (board_units(index) + 0.5).mul_add(cell, board.left);
        if (selected || is_labelled_coordinate(index, size, stride))
            && (viewport.left..=viewport.right()).contains(&x)
        {
            labels.push(BoardText {
                x: x - label_spacing / 2.0,
                y: viewport.top - 12.0 - half_height,
                width: label_spacing,
                text: (index + 1).to_string().into(),
                color: color(selected),
                size: COORDINATE_FONT,
                right: false,
            });
        }
    }
    for index in visible_cells(board.top, cell, viewport.top, viewport.bottom(), size) {
        let selected = selected_row == Some(index);
        let y = (board_units(index) + 0.5).mul_add(cell, board.top);
        if (selected || is_labelled_coordinate(index, size, stride))
            && (viewport.top..=viewport.bottom()).contains(&y)
        {
            labels.push(BoardText {
                x: 0.0,
                y: y - half_height,
                width: viewport.left - 6.0,
                text: (index + 1).to_string().into(),
                color: color(selected),
                size: COORDINATE_FONT,
                right: true,
            });
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(buffer: &SharedPixelBuffer<Rgba8Pixel>, x: usize, y: usize) -> [u8; 4] {
        let pixel = buffer.as_slice()[y * buffer.width() as usize + x];
        [pixel.r, pixel.g, pixel.b, pixel.a]
    }

    fn rgba(color: Rgba) -> [u8; 4] {
        [color.r, color.g, color.b, color.a]
    }

    #[test]
    fn viewports_leave_a_gutter_and_stay_square() {
        let board = viewport(Some(8), 600.0, 400.0);
        assert_eq!(board.top, 28.0);
        assert_eq!(board.side, 372.0);
        assert!(board.left > 28.0, "centred horizontally");
        let empty = viewport(None, 100.0, 100.0);
        assert_eq!(empty.side, MIN_BOARD_SIDE);
    }

    #[test]
    fn queens_are_drawn_in_their_role_colors() {
        let palette = &Palette::DARK;
        let positions = [1, 3, 0, 2];
        let scene = Scene {
            positions: &positions,
            conflicts: &[0, 0, 0, 0],
            selected: None,
            view: BoardView::default(),
            palette,
        };
        let buffer = render(&scene, 400.0, 1.0);
        assert_eq!(buffer.width(), 400);
        // Column 0's queen is in row 1, centred at (50, 150); it has no attackers.
        assert_eq!(pixel(&buffer, 50, 150), rgba(palette.success));
        // The square at column 0, row 0 is light; column 1, row 0 is dark.
        assert_eq!(pixel(&buffer, 5, 5), rgba(palette.light_square));
        assert_eq!(pixel(&buffer, 105, 5), rgba(palette.dark_square));

        let scene = Scene {
            selected: Some(0),
            ..scene
        };
        let buffer = render(&scene, 400.0, 2.0);
        assert_eq!(buffer.width(), 800);
        assert_eq!(pixel(&buffer, 100, 300), rgba(palette.selected_queen));
        // Column 1's queen does not attack column 0's, so it is dimmed.
        assert_eq!(pixel(&buffer, 300, 700), rgba(palette.queen_dimmed));
    }

    #[test]
    fn huge_boards_draw_every_queen_as_pixels() {
        let positions: Vec<u16> = (0..u16::MAX).collect();
        let conflicts = vec![1; positions.len()];
        let scene = Scene {
            positions: &positions,
            conflicts: &conflicts,
            selected: Some(100),
            view: BoardView::default(),
            palette: &Palette::LIGHT,
        };
        let buffer = render(&scene, 500.0, 2.0);
        // The diagonal of queens runs through the middle of the board.
        assert_ne!(pixel(&buffer, 500, 500), rgba(Palette::LIGHT.sparse_board));
        assert_eq!(
            queen_labels(&scene, viewport(Some(positions.len()), 500.0, 500.0)),
            []
        );
    }

    #[test]
    fn a_huge_board_at_full_zoom_lays_out_only_what_is_in_view() {
        let positions: Vec<u16> = (0..u16::MAX).collect();
        let conflicts = vec![u32::from(u16::MAX) - 1; positions.len()];
        let mut view = BoardView::default();
        view.set_zoom(crate::gui::board::MAX_ZOOM, Point::new(0.5, 0.5));
        let scene = Scene {
            positions: &positions,
            conflicts: &conflicts,
            selected: Some(32_000),
            view,
            palette: &Palette::DARK,
        };
        let viewport = viewport(Some(positions.len()), 800.0, 800.0);
        let buffer = render(&scene, viewport.side, 2.0);
        assert_eq!(buffer.width(), pixels((viewport.side * 2.0).round()));
        let labels = queen_labels(&scene, viewport);
        assert!(labels.len() < 100, "{} queen labels", labels.len());
        let coordinates = coordinate_labels(&scene, viewport);
        assert!(!coordinates.is_empty() && coordinates.len() < 100);
    }

    #[test]
    fn labels_follow_zoom_and_the_selection() {
        let positions: Vec<u16> = (0..18).collect();
        let conflicts = vec![17; 18];
        let mut view = BoardView::default();
        let scene = Scene {
            positions: &positions,
            conflicts: &conflicts,
            selected: Some(4),
            view,
            palette: &Palette::DARK,
        };
        let viewport = viewport(Some(18), 600.0, 600.0);
        let labels = queen_labels(&scene, viewport);
        assert_eq!(labels.len(), 18);
        assert_eq!(labels[0].text, "17");
        let coordinates = coordinate_labels(&scene, viewport);
        let selected: Vec<_> = coordinates
            .iter()
            .filter(|label| label.color == slint_color(Palette::DARK.selected_queen))
            .map(|label| label.text.as_str())
            .collect();
        assert_eq!(selected, ["5", "5"]);

        view.set_zoom(4.0, Point::new(0.0, 0.0));
        let zoomed = Scene { view, ..scene };
        let labels = queen_labels(&zoomed, viewport);
        assert!(labels.len() < 8, "only queens in view are labelled");
        assert!(
            coordinate_labels(&zoomed, viewport)
                .iter()
                .all(|label| label.text.parse::<usize>().unwrap() <= 6)
        );
    }
}
