//! Board geometry and text: zoom and pan, hit testing, attackers, and what the
//! board says about a queen. Drawing is left to the toolkit.

use super::{attacking_pairs, palette::Palette, palette::Rgba};

/// The board never shrinks below this side, in points.
pub(super) const MIN_BOARD_SIDE: f32 = 160.0;
/// Room below the board for the selection details, so selecting a queen never resizes it.
pub(super) const SELECTION_INFO_HEIGHT: f32 = 44.0;
/// Below this cell side, in points, the board draws a sparse grid instead of squares.
pub(super) const MIN_SQUARE_CELL: f32 = 6.0;
/// Below this cell side, queens carry no attacker count.
pub(super) const MIN_LABELLED_CELL: f32 = 16.0;
pub(super) const MAX_ZOOM: f32 = 1024.0;
/// Each zoom button press scales by this factor.
pub(super) const ZOOM_STEP: f32 = 1.5;
/// Attackers listed by name before the rest are counted.
const LISTED_ATTACKERS: usize = 8;

pub(super) const EMPTY_BOARD_TEXT: &str = "Run the solver to draw a board";
pub(super) const HELP_TEXT: [&[&str]; 4] = [
    &["A number on a queen counts the queens attacking it; Q marks a queen nobody attacks."],
    &[
        "Click a queen to inspect its conflicts. Click it again or an empty square to clear.",
        "Press Escape while the board is focused to clear the selection. Otherwise Escape cancels a running solver.",
    ],
    &[
        "Drag to pan · Ctrl/Cmd + scroll or pinch to zoom.",
        "Fit resets the zoom and pan to show the whole board.",
    ],
    &["Coordinates are 1-based: columns left to right, rows top to bottom."],
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub(super) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned square, the only shape a board or its viewport takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Square {
    pub left: f32,
    pub top: f32,
    pub side: f32,
}

impl Square {
    pub(super) fn right(self) -> f32 {
        self.left + self.side
    }

    pub(super) fn bottom(self) -> f32 {
        self.top + self.side
    }
}

/// Pan is measured in viewport widths, so resizing preserves the viewed board region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoardView {
    zoom: f32,
    pan: Point,
}

impl Default for BoardView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Point::default(),
        }
    }
}

impl BoardView {
    pub(super) const fn zoom(self) -> f32 {
        self.zoom
    }

    /// Zooms around `anchor`, given as a fraction of the viewport, which stays put on screen.
    pub(super) fn set_zoom(&mut self, zoom: f32, anchor: Point) {
        if !zoom.is_finite() || !anchor.x.is_finite() || !anchor.y.is_finite() {
            return;
        }
        let next = zoom.clamp(1.0, MAX_ZOOM);
        let scale = next / self.zoom;
        self.pan = Point::new(
            (anchor.x - self.pan.x).mul_add(-scale, anchor.x),
            (anchor.y - self.pan.y).mul_add(-scale, anchor.y),
        );
        self.zoom = next;
        self.clamp_pan();
    }

    pub(super) fn zoom_in(&mut self) {
        self.set_zoom(self.zoom * ZOOM_STEP, Point::new(0.5, 0.5));
    }

    pub(super) fn zoom_out(&mut self) {
        self.set_zoom(self.zoom / ZOOM_STEP, Point::new(0.5, 0.5));
    }

    pub(super) fn can_zoom_in(self) -> bool {
        self.zoom < MAX_ZOOM
    }

    pub(super) fn can_zoom_out(self) -> bool {
        self.zoom > 1.0
    }

    pub(super) fn zoom_label(self) -> String {
        format!("{:.0}%", self.zoom * 100.0)
    }

    /// Pans by `delta`, given as a fraction of the viewport.
    pub(super) fn move_by(&mut self, delta: Point) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        self.pan = Point::new(self.pan.x + delta.x, self.pan.y + delta.y);
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        let min = 1.0 - self.zoom;
        self.pan = Point::new(self.pan.x.clamp(min, 0.0), self.pan.y.clamp(min, 0.0));
    }

    /// Where the whole board lies, zoomed and panned, for a board shown in `viewport`.
    pub(super) fn board(self, viewport: Square) -> Square {
        Square {
            left: self.pan.x.mul_add(viewport.side, viewport.left),
            top: self.pan.y.mul_add(viewport.side, viewport.top),
            side: viewport.side * self.zoom,
        }
    }
}

// A board dimension and its coordinate indices fit the solver's u16 range.
pub(super) fn board_units(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("board coordinate fits u16"))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Screen-space indices intentionally round down; conversion saturates and callers bound indices to the board."
)]
pub(super) const fn cell_index(value: f32) -> usize {
    value.max(0.0) as usize
}

/// The cells, out of `size`, that overlap `min..max` when cells of side `cell` start at `start`.
pub(super) fn visible_cells(
    start: f32,
    cell: f32,
    min: f32,
    max: f32,
    size: usize,
) -> std::ops::Range<usize> {
    let first = cell_index(((min - start) / cell).floor()).min(size);
    let end = cell_index(((max - start) / cell).ceil()).min(size);
    first..end.max(first)
}

/// Spacing between coordinate labels and the gutter that fits the widest row label,
/// in points, so long numbers never collide.
pub(super) fn label_spacing_and_gutter(size: usize) -> (f32, f32) {
    let digits = board_units(size.to_string().len());
    let label_spacing = 7.0f32.mul_add(digits, 6.0).max(24.0);
    let gutter = 7.0f32.mul_add(digits, 8.0).max(28.0);
    (label_spacing, gutter)
}

/// Every how many coordinates a label fits, for cells of side `cell`.
pub(super) fn label_stride(label_spacing: f32, cell: f32) -> usize {
    cell_index((label_spacing / cell).ceil()).max(1)
}

/// Every how many lines the sparse grid draws one, for cells of side `cell`.
pub(super) fn sparse_grid_stride(cell: f32) -> usize {
    cell_index((24.0 / cell).ceil()).max(1)
}

/// Positions are a permutation, so only diagonals can contain attacking queens.
pub(super) fn attacking_columns(positions: &[u16], selected: usize) -> Vec<usize> {
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

/// Which of the selected queen's diagonals hold an attacker: `[descending, ascending]`,
/// where descending runs from the top left to the bottom right.
pub(super) fn attacked_diagonals(
    positions: &[u16],
    selected: usize,
    attackers: &[usize],
) -> [bool; 2] {
    let Some(&row) = positions.get(selected) else {
        return [false; 2];
    };
    let row = usize::from(row);
    [true, false].map(|descending| {
        attackers.iter().any(|&other| {
            ((other > selected) == (usize::from(positions[other]) > row)) == descending
        })
    })
}

pub(super) fn queen_at_pointer(board: Square, positions: &[u16], pointer: Point) -> Option<usize> {
    if positions.is_empty()
        || board.side <= 0.0
        || pointer.x < board.left
        || pointer.x >= board.right()
        || pointer.y < board.top
        || pointer.y >= board.bottom()
    {
        return None;
    }
    let size = board_units(positions.len());
    let column = cell_index((pointer.x - board.left) / board.side * size);
    let row = cell_index((pointer.y - board.top) / board.side * size);
    positions
        .get(column)
        .filter(|&&queen_row| usize::from(queen_row) == row)
        .map(|_| column)
}

pub(super) fn queen_center(board: Square, size: usize, column: usize, row: usize) -> Point {
    let cell = board.side / board_units(size);
    Point::new(
        (board_units(column) + 0.5).mul_add(cell, board.left),
        (board_units(row) + 0.5).mul_add(cell, board.top),
    )
}

/// Endpoints of a full diagonal, clipped to the board's outer edges.
pub(super) fn diagonal_segment(board: Square, center: Point, descending: bool) -> [Point; 2] {
    if descending {
        let before = (center.x - board.left).min(center.y - board.top);
        let after = (board.right() - center.x).min(board.bottom() - center.y);
        [
            Point::new(center.x - before, center.y - before),
            Point::new(center.x + after, center.y + after),
        ]
    } else {
        let before = (center.x - board.left).min(board.bottom() - center.y);
        let after = (board.right() - center.x).min(center.y - board.top);
        [
            Point::new(center.x - before, center.y + before),
            Point::new(center.x + after, center.y - after),
        ]
    }
}

pub(super) fn queen_radius(cell: f32) -> f32 {
    (cell * 0.34).clamp(2.0, 18.0)
}

pub(super) fn diagonal_band_width(cell: f32) -> f32 {
    (cell * 0.24).clamp(3.0, 10.0)
}

/// Side of the square outline drawn around an attacker.
pub(super) fn attacker_outline_side(cell: f32) -> f32 {
    (cell * 0.88).max(5.0)
}

pub(super) fn queen_font_size(cell: f32) -> f32 {
    (cell * 0.42).clamp(10.0, 24.0)
}

/// Every `stride`-th coordinate is labelled, plus the last one; a sampled label
/// closer than `stride` to the last is dropped so the two never overlap.
pub(super) const fn is_labelled_coordinate(index: usize, size: usize, stride: usize) -> bool {
    index + 1 == size || (index.is_multiple_of(stride) && size - 1 - index >= stride)
}

/// The attacker count, so conflicts read without relying on color; "Q" when there are none.
/// Longer counts get a smaller font, and labels that still overflow the queen are skipped.
pub(super) fn queen_label(attackers: u32, font_size: f32, radius: f32) -> Option<(String, f32)> {
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

/// How a queen is drawn, given the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QueenRole {
    Selected,
    /// Attacks the selected queen; also outlined so it does not rely on color alone.
    Attacker,
    /// Neither selected nor attacking while a queen is selected.
    Dimmed,
    /// No selection: colored by its attacker count.
    Normal {
        conflicts: u32,
    },
}

impl QueenRole {
    pub(super) const fn of(
        column: usize,
        selected: Option<usize>,
        is_attacker: bool,
        conflicts: u32,
    ) -> Self {
        match selected {
            Some(selected) if selected == column => Self::Selected,
            Some(_) if is_attacker => Self::Attacker,
            Some(_) => Self::Dimmed,
            None => Self::Normal { conflicts },
        }
    }

    pub(super) const fn fill(self, palette: &Palette) -> Rgba {
        match self {
            Self::Selected => palette.selected_queen,
            Self::Attacker => palette.attacker,
            Self::Dimmed => palette.queen_dimmed,
            Self::Normal { conflicts } => palette.queen(conflicts),
        }
    }
}

/// The heading line next to "Board".
pub(super) fn board_summary(size: usize, conflicts_sum: u32) -> String {
    format!("{size} queens · {}", attacking_pairs(conflicts_sum))
}

pub(super) fn hover_text(positions: &[u16], conflicts: &[u32], column: usize) -> String {
    let count = conflicts.get(column).copied().unwrap_or_default();
    format!(
        "Column {}, row {}\n{} attacking queen{}\nClick to inspect",
        column + 1,
        usize::from(positions[column]) + 1,
        count,
        if count == 1 { "" } else { "s" }
    )
}

/// The selected queen's coordinates, and who attacks it.
pub(super) fn selection_text(
    positions: &[u16],
    selected: usize,
    attackers: &[usize],
) -> (String, String) {
    let title = format!(
        "Selected: column {}, row {}",
        selected + 1,
        usize::from(positions[selected]) + 1
    );
    if attackers.is_empty() {
        return (title, "This queen has no conflicts.".to_owned());
    }
    let coordinates = attackers
        .iter()
        .take(LISTED_ATTACKERS)
        .map(|&other| format!("C{} R{}", other + 1, usize::from(positions[other]) + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let more = if attackers.len() > LISTED_ATTACKERS {
        format!(" and {} more", attackers.len() - LISTED_ATTACKERS)
    } else {
        String::new()
    };
    (
        title,
        format!(
            "Attacked by {} queen{}: {coordinates}{more}. Highlighted lines show their shared diagonals.",
            attackers.len(),
            if attackers.len() == 1 { "" } else { "s" }
        ),
    )
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "These tests check clamped zoom limits, which are exact."
)]
mod tests {
    use super::*;
    use crate::ga;

    fn square(left: f32, top: f32, side: f32) -> Square {
        Square { left, top, side }
    }

    #[test]
    fn board_zoom_preserves_pointer_anchor_and_pan_is_bounded() {
        let viewport = square(10.0, 20.0, 400.0);
        let positions = [1, 3, 0, 2];
        let mut view = BoardView::default();
        let point = queen_center(viewport, 4, 2, 0);
        let anchor = Point::new(
            (point.x - viewport.left) / viewport.side,
            (point.y - viewport.top) / viewport.side,
        );
        view.set_zoom(4.0, anchor);
        assert_eq!(
            queen_at_pointer(view.board(viewport), &positions, point),
            Some(2)
        );
        view.move_by(Point::new(100.0, 100.0));
        assert_eq!(view.pan, Point::default());
        view.move_by(Point::new(-100.0, -100.0));
        assert_eq!(view.pan, Point::new(-3.0, -3.0));
        let before = view;
        view.set_zoom(f32::NAN, anchor);
        assert_eq!(view, before);
        view.set_zoom(1.0, anchor);
        assert_eq!(view, BoardView::default());
        view.set_zoom(100_000.0, anchor);
        assert_eq!(view.zoom(), MAX_ZOOM);
        assert!(!view.can_zoom_in());
        view = BoardView::default();
        assert!(!view.can_zoom_out());
        view.zoom_in();
        assert_eq!(view.zoom_label(), "150%");
        view.zoom_out();
        assert_eq!(view, BoardView::default());
    }

    #[test]
    fn visible_cell_range_limits_work_on_large_zoomed_boards() {
        let range = visible_cells(-9_900.0, 10.0, 0.0, 400.0, 65_535);
        assert_eq!(range, 990..1030);
        assert_eq!(visible_cells(500.0, 10.0, 0.0, 400.0, 100), 0..0);
        assert_eq!(visible_cells(0.0, 10.0, 0.0, 1000.0, 4), 0..4);
    }

    #[test]
    fn labels_and_gutter_fit_the_widest_coordinate() {
        assert_eq!(label_spacing_and_gutter(8), (24.0, 28.0));
        assert_eq!(label_spacing_and_gutter(65_535), (41.0, 43.0));
        assert_eq!(label_stride(24.0, 30.0), 1);
        assert_eq!(label_stride(41.0, 4.0), 11);
        assert_eq!(sparse_grid_stride(5.0), 5);
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
    fn only_diagonals_holding_an_attacker_are_highlighted() {
        // Columns 0 and 4 share column 2's descending diagonal; columns 1 and 3 its ascending one.
        let positions = [0, 3, 2, 1, 4];
        let attackers = attacking_columns(&positions, 2);
        assert_eq!(attackers, [0, 1, 3, 4]);
        assert_eq!(attacked_diagonals(&positions, 2, &attackers), [true, true]);
        assert_eq!(attacked_diagonals(&positions, 2, &[0, 4]), [true, false]);
        assert_eq!(attacked_diagonals(&positions, 2, &[1]), [false, true]);
        assert_eq!(attacked_diagonals(&positions, 2, &[]), [false, false]);
        assert_eq!(attacked_diagonals(&positions, 9, &[]), [false, false]);
    }

    #[test]
    fn board_hit_testing_distinguishes_queens_empty_squares_and_edges() {
        let board = square(10.0, 20.0, 80.0);
        let positions = [1, 3, 0, 2];
        for (column, &row) in positions.iter().enumerate() {
            assert_eq!(
                queen_at_pointer(
                    board,
                    &positions,
                    queen_center(board, 4, column, row as usize)
                ),
                Some(column)
            );
        }
        for point in [
            Point::new(10.0, 20.0),
            Point::new(9.0, 50.0),
            Point::new(90.0, 50.0),
            Point::new(50.0, 100.0),
        ] {
            assert_eq!(queen_at_pointer(board, &positions, point), None);
        }
        assert_eq!(queen_at_pointer(board, &[], Point::new(50.0, 60.0)), None);
        assert_eq!(
            queen_at_pointer(board, &[0], Point::new(10.0, 20.0)),
            Some(0)
        );
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
        let board = square(10.0, 20.0, 80.0);
        let center = Point::new(50.0, 60.0);
        assert_eq!(
            diagonal_segment(board, center, true),
            [Point::new(10.0, 20.0), Point::new(90.0, 100.0)]
        );
        assert_eq!(
            diagonal_segment(board, center, false),
            [Point::new(10.0, 100.0), Point::new(90.0, 20.0)]
        );
        let center = Point::new(20.0, 50.0);
        assert_eq!(
            diagonal_segment(board, center, true),
            [Point::new(10.0, 40.0), Point::new(70.0, 100.0)]
        );
        assert_eq!(
            diagonal_segment(board, center, false),
            [Point::new(10.0, 60.0), Point::new(50.0, 20.0)]
        );
    }

    #[test]
    fn queens_are_drawn_by_their_role_in_the_selection() {
        let palette = &Palette::DARK;
        assert_eq!(QueenRole::of(1, Some(1), false, 3), QueenRole::Selected);
        assert_eq!(QueenRole::of(2, Some(1), true, 3), QueenRole::Attacker);
        assert_eq!(QueenRole::of(3, Some(1), false, 3), QueenRole::Dimmed);
        assert_eq!(
            QueenRole::of(3, None, false, 0).fill(palette),
            palette.success
        );
        assert_eq!(
            QueenRole::of(3, None, false, 3).fill(palette),
            palette.danger
        );
    }

    #[test]
    fn selection_text_lists_up_to_eight_attackers() {
        let positions: Vec<u16> = (0..12).collect();
        let attackers = attacking_columns(&positions, 0);
        let (title, details) = selection_text(&positions, 0, &attackers);
        assert_eq!(title, "Selected: column 1, row 1");
        assert!(
            details.starts_with("Attacked by 11 queens: C2 R2, C3 R3,"),
            "{details}"
        );
        assert!(details.contains("C9 R9 and 3 more."), "{details}");
        let (_, details) = selection_text(&[1, 3, 0, 2], 0, &[]);
        assert_eq!(details, "This queen has no conflicts.");
        assert_eq!(
            hover_text(&[1, 3, 0, 2], &[0, 1, 0, 0], 1),
            "Column 2, row 4\n1 attacking queen\nClick to inspect"
        );
        assert_eq!(board_summary(4, 1), "4 queens · 1 attacking pair");
    }
}
