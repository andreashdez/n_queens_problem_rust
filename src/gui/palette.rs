//! Every color the GUI paints itself, in a dark and a light variant. Widgets and
//! plots take their colors from the toolkit's theme; these cover everything drawn on top.

/// An sRGB color with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub(super) const WHITE: Self = Self::rgb(255, 255, 255);
    pub(super) const GRAY: Self = Self::rgb(160, 160, 160);

    pub(super) const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub(super) const fn translucent(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

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
    pub success: Rgba,
    pub warning: Rgba,
    pub danger: Rgba,
    /// Readable as text, unlike [`Self::danger`] on a dark background.
    pub error_text: Rgba,
    /// Fill of the primary action button; its label uses the theme's text color.
    pub accent: Rgba,

    // Board.
    pub empty_board: Rgba,
    pub empty_board_text: Rgba,
    pub dark_square: Rgba,
    pub light_square: Rgba,
    /// Background when the board is too dense to draw individual squares.
    pub sparse_board: Rgba,
    pub sparse_grid_line: Rgba,
    pub coordinate: Rgba,
    /// Text on a queen, so it must contrast with every queen fill.
    pub queen_label: Rgba,
    /// Queens that are neither selected nor attacking while a queen is selected.
    pub queen_dimmed: Rgba,
    pub selected_queen: Rgba,
    pub selection_ring: Rgba,
    pub attacker: Rgba,
    /// Wide translucent band under each attacked diagonal.
    pub attack_diagonal: Rgba,

    // Chart lines that have no outcome meaning.
    pub average_conflicts: Rgba,
    pub diversity: Rgba,
    pub mutation_rate: Rgba,
    pub elite_fraction: Rgba,
}

impl Palette {
    pub(super) const DARK: Self = Self {
        success: Rgba::rgb(95, 220, 140),
        warning: Rgba::rgb(245, 190, 85),
        danger: Rgba::rgb(245, 95, 105),
        error_text: Rgba::rgb(255, 120, 120),
        accent: Rgba::rgb(36, 91, 122),

        empty_board: Rgba::rgb(18, 24, 34),
        empty_board_text: Rgba::rgb(180, 190, 205),
        dark_square: Rgba::rgb(25, 32, 44),
        light_square: Rgba::rgb(39, 52, 68),
        sparse_board: Rgba::rgb(19, 25, 35),
        sparse_grid_line: Rgba::translucent(120, 210, 220, 45),
        coordinate: Rgba::GRAY,
        queen_label: Rgba::rgb(12, 16, 22),
        queen_dimmed: Rgba::rgb(80, 99, 114),
        selected_queen: Rgba::rgb(105, 195, 255),
        selection_ring: Rgba::WHITE,
        attacker: Rgba::rgb(255, 177, 86),
        attack_diagonal: Rgba::translucent(255, 177, 86, 50),

        average_conflicts: Rgba::rgb(110, 190, 255),
        diversity: Rgba::rgb(245, 210, 95),
        mutation_rate: Rgba::rgb(245, 120, 170),
        elite_fraction: Rgba::rgb(160, 135, 255),
    };

    /// Darker, more saturated tones: at least 4.5:1 as text on light panels,
    /// and white queen labels stay readable on every queen fill.
    pub(super) const LIGHT: Self = Self {
        success: Rgba::rgb(22, 128, 62),
        warning: Rgba::rgb(166, 96, 0),
        danger: Rgba::rgb(200, 40, 55),
        error_text: Rgba::rgb(190, 30, 40),
        accent: Rgba::rgb(160, 205, 240),

        empty_board: Rgba::rgb(228, 233, 240),
        empty_board_text: Rgba::rgb(85, 95, 110),
        dark_square: Rgba::rgb(206, 214, 225),
        light_square: Rgba::rgb(236, 240, 245),
        sparse_board: Rgba::rgb(232, 236, 242),
        sparse_grid_line: Rgba::translucent(40, 90, 120, 60),
        coordinate: Rgba::rgb(105, 105, 105),
        queen_label: Rgba::WHITE,
        queen_dimmed: Rgba::rgb(125, 135, 148),
        selected_queen: Rgba::rgb(20, 105, 190),
        selection_ring: Rgba::rgb(25, 30, 40),
        attacker: Rgba::rgb(180, 88, 0),
        attack_diagonal: Rgba::translucent(180, 88, 0, 60),

        average_conflicts: Rgba::rgb(30, 115, 205),
        diversity: Rgba::rgb(176, 128, 0),
        mutation_rate: Rgba::rgb(200, 50, 120),
        elite_fraction: Rgba::rgb(105, 75, 215),
    };

    pub(super) const fn of(dark_mode: bool) -> &'static Self {
        if dark_mode { &Self::DARK } else { &Self::LIGHT }
    }

    pub(super) const fn tone(&self, tone: Tone) -> Rgba {
        match tone {
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Error => self.error_text,
        }
    }

    pub(super) const fn queen(&self, conflicts: u32) -> Rgba {
        match conflicts {
            0 => self.success,
            1 | 2 => self.warning,
            _ => self.danger,
        }
    }

    pub(super) const fn best_conflicts(&self) -> Rgba {
        self.success
    }

    pub(super) const fn restart(&self) -> Rgba {
        self.warning
    }

    /// Comparison runs reuse the board's selection colors.
    pub(super) const fn compare(&self) -> [Rgba; 2] {
        [self.selected_queen, self.attacker]
    }
}

/// WCAG 2 contrast ratio between two opaque colors.
#[cfg(test)]
pub(super) fn contrast(a: Rgba, b: Rgba) -> f32 {
    let luminance = |color: Rgba| {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.0722f32.mul_add(
            channel(color.b),
            0.2126f32.mul_add(channel(color.r), 0.7152 * channel(color.g)),
        )
    };
    let (light, dark) = {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b), a.min(b))
    };
    (light + 0.05) / (dark + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queen_labels_stay_readable_on_every_queen_fill() {
        for (palette, name) in [(&Palette::DARK, "dark"), (&Palette::LIGHT, "light")] {
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
        }
        assert_eq!(Palette::of(true).queen_label, Palette::DARK.queen_label);
        assert_eq!(Palette::of(false).queen_label, Palette::LIGHT.queen_label);
    }
}
