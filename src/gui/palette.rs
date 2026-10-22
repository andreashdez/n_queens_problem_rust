//! Every color the GUI paints itself, in a dark and a light variant. Widgets and
//! plots take their colors from egui's visuals; these cover everything drawn on top.

use eframe::egui::{self, Color32};

/// How an outcome should read; drawn in the active palette's color for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Success,
    Warning,
    Error,
}

#[derive(Debug)]
pub(super) struct Palette {
    // Outcome tones, shared by status text, queens and chart lines so that green,
    // amber and red mean the same thing everywhere.
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    /// Readable as text, unlike [`Self::danger`] on a dark background.
    pub error_text: Color32,
    /// Fill of the primary action button; its label uses the theme's text color.
    pub accent: Color32,

    // Board.
    pub empty_board: Color32,
    pub empty_board_text: Color32,
    pub dark_square: Color32,
    pub light_square: Color32,
    /// Background when the board is too dense to draw individual squares.
    pub sparse_board: Color32,
    pub sparse_grid_line: Color32,
    pub coordinate: Color32,
    /// Text on a queen, so it must contrast with every queen fill.
    pub queen_label: Color32,
    /// Queens that are neither selected nor attacking while a queen is selected.
    pub queen_dimmed: Color32,
    pub selected_queen: Color32,
    pub selection_ring: Color32,
    pub attacker: Color32,
    /// Wide translucent band under each attacked diagonal.
    pub attack_diagonal: Color32,

    // Chart lines that have no outcome meaning.
    pub average_conflicts: Color32,
    pub diversity: Color32,
    pub mutation_rate: Color32,
    pub elite_fraction: Color32,
}

impl Palette {
    pub(super) const DARK: Self = Self {
        success: Color32::from_rgb(95, 220, 140),
        warning: Color32::from_rgb(245, 190, 85),
        danger: Color32::from_rgb(245, 95, 105),
        error_text: Color32::from_rgb(255, 120, 120),
        accent: Color32::from_rgb(36, 91, 122),

        empty_board: Color32::from_rgb(18, 24, 34),
        empty_board_text: Color32::from_rgb(180, 190, 205),
        dark_square: Color32::from_rgb(25, 32, 44),
        light_square: Color32::from_rgb(39, 52, 68),
        sparse_board: Color32::from_rgb(19, 25, 35),
        sparse_grid_line: Color32::from_rgba_unmultiplied_const(120, 210, 220, 45),
        coordinate: Color32::GRAY,
        queen_label: Color32::from_rgb(12, 16, 22),
        queen_dimmed: Color32::from_rgb(80, 99, 114),
        selected_queen: Color32::from_rgb(105, 195, 255),
        selection_ring: Color32::WHITE,
        attacker: Color32::from_rgb(255, 177, 86),
        attack_diagonal: Color32::from_rgba_unmultiplied_const(255, 177, 86, 50),

        average_conflicts: Color32::from_rgb(110, 190, 255),
        diversity: Color32::from_rgb(245, 210, 95),
        mutation_rate: Color32::from_rgb(245, 120, 170),
        elite_fraction: Color32::from_rgb(160, 135, 255),
    };

    /// Darker, more saturated tones: at least 4.5:1 as text on light panels,
    /// and white queen labels stay readable on every queen fill.
    pub(super) const LIGHT: Self = Self {
        success: Color32::from_rgb(22, 128, 62),
        warning: Color32::from_rgb(166, 96, 0),
        danger: Color32::from_rgb(200, 40, 55),
        error_text: Color32::from_rgb(190, 30, 40),
        accent: Color32::from_rgb(160, 205, 240),

        empty_board: Color32::from_rgb(228, 233, 240),
        empty_board_text: Color32::from_rgb(85, 95, 110),
        dark_square: Color32::from_rgb(206, 214, 225),
        light_square: Color32::from_rgb(236, 240, 245),
        sparse_board: Color32::from_rgb(232, 236, 242),
        sparse_grid_line: Color32::from_rgba_unmultiplied_const(40, 90, 120, 60),
        coordinate: Color32::from_rgb(105, 105, 105),
        queen_label: Color32::WHITE,
        queen_dimmed: Color32::from_rgb(125, 135, 148),
        selected_queen: Color32::from_rgb(20, 105, 190),
        selection_ring: Color32::from_rgb(25, 30, 40),
        attacker: Color32::from_rgb(180, 88, 0),
        attack_diagonal: Color32::from_rgba_unmultiplied_const(180, 88, 0, 60),

        average_conflicts: Color32::from_rgb(30, 115, 205),
        diversity: Color32::from_rgb(176, 128, 0),
        mutation_rate: Color32::from_rgb(200, 50, 120),
        elite_fraction: Color32::from_rgb(105, 75, 215),
    };

    /// The palette for the theme egui is currently drawing with.
    pub(super) fn of(ctx: &egui::Context) -> &'static Self {
        if ctx.global_style().visuals.dark_mode {
            &Self::DARK
        } else {
            &Self::LIGHT
        }
    }

    pub(super) const fn tone(&self, tone: Tone) -> Color32 {
        match tone {
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Error => self.error_text,
        }
    }

    pub(super) const fn queen(&self, conflicts: u32) -> Color32 {
        match conflicts {
            0 => self.success,
            1 | 2 => self.warning,
            _ => self.danger,
        }
    }

    pub(super) const fn best_conflicts(&self) -> Color32 {
        self.success
    }

    pub(super) const fn restart(&self) -> Color32 {
        self.warning
    }

    /// Comparison runs reuse the board's selection colors.
    pub(super) const fn compare(&self) -> [Color32; 2] {
        [self.selected_queen, self.attacker]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 contrast ratio between two opaque colors.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let luminance = |color: Color32| {
            let channel = |value: u8| {
                let value = f32::from(value) / 255.0;
                if value <= 0.039_28 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.0722f32.mul_add(
                channel(color.b()),
                0.2126f32.mul_add(channel(color.r()), 0.7152 * channel(color.g())),
            )
        };
        let (light, dark) = {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b), a.min(b))
        };
        (light + 0.05) / (dark + 0.05)
    }

    #[test]
    fn both_themes_keep_text_and_queens_readable() {
        for (palette, visuals) in [
            (&Palette::DARK, egui::Visuals::dark()),
            (&Palette::LIGHT, egui::Visuals::light()),
        ] {
            let name = if visuals.dark_mode { "dark" } else { "light" };
            for (text, label) in [
                (palette.success, "success"),
                (palette.warning, "warning"),
                (palette.error_text, "error"),
                (palette.selected_queen, "selected queen"),
                (palette.coordinate, "coordinates"),
            ] {
                let ratio = contrast(text, visuals.panel_fill);
                assert!(ratio >= 4.5, "{name} {label} text: {ratio:.2}");
            }
            for (fill, label) in [
                (palette.success, "safe"),
                (palette.warning, "few conflicts"),
                (palette.danger, "many conflicts"),
                (palette.selected_queen, "selected"),
                (palette.attacker, "attacker"),
                (palette.queen_dimmed, "dimmed"),
            ] {
                let ratio = contrast(palette.queen_label, fill);
                assert!(ratio >= 3.0, "{name} label on {label} queen: {ratio:.2}");
            }
            assert!(Palette::of(&context_with(visuals)).queen_label == palette.queen_label);
        }
    }

    fn context_with(visuals: egui::Visuals) -> egui::Context {
        let ctx = egui::Context::default();
        let theme = if visuals.dark_mode {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(theme);
        ctx
    }
}
