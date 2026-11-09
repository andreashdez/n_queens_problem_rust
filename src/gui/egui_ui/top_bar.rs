//! The top bar: the one place for run actions, live progress, and the outcome.

use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText, ThemePreference, Vec2};

use super::{now, palette};
use crate::gui::state::AppState;

const RUN_BUTTON_SIZE: Vec2 = Vec2::new(120.0, 28.0);

impl AppState {
    /// The one place for run actions, live progress, and the outcome.
    pub(super) fn draw_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = palette(ctx);
        ui.horizontal(|ui| {
            if let Some((fraction, text, step)) = self.run_progress() {
                if ui
                    .add_enabled(
                        !self.cancel_requested,
                        egui::Button::new(if self.cancel_requested {
                            "Cancelling…"
                        } else {
                            "Cancel run"
                        })
                        .min_size(RUN_BUTTON_SIZE),
                    )
                    .on_hover_text(format!("Stop after the current {} (Esc)", step.one))
                    .clicked()
                {
                    self.cancel_run();
                }
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(text)
                        .desired_width(260.0),
                )
                .on_hover_text(format!(
                    "Share of the {} budget used; a solution can stop the run early.",
                    step.one
                ));
            } else if ui
                .add(
                    egui::Button::new(RichText::new("Run solver").strong())
                        .fill(palette.accent)
                        .min_size(RUN_BUTTON_SIZE),
                )
                .on_hover_text("Start a run with the settings on the left (Ctrl/Cmd+Enter)")
                .clicked()
            {
                self.start_run();
            }
            ui.separator();
            let (text, tone) = self.status();
            let text = RichText::new(text);
            ui.label(match tone {
                Some(tone) => text.color(palette.tone(tone)),
                None => text,
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                draw_theme_choice(ui);
                ui.separator();
                ui.toggle_value(
                    &mut self.show_history,
                    format!("Run history ({})", self.archive.len()),
                )
                .on_hover_text("Show or hide the run history panel");
                self.draw_notice(ui);
            });
        });
    }

    /// Shows the latest notice until it expires.
    fn draw_notice(&mut self, ui: &mut egui::Ui) {
        let now = now(ui.ctx());
        let Some(notice) = self.current_notice(now) else {
            return;
        };
        ui.separator();
        let text = RichText::new(&notice.text);
        ui.label(if notice.is_error {
            text.color(palette(ui.ctx()).error_text)
        } else {
            text.weak()
        });
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f64(notice.expires_at - now));
    }
}

/// Text buttons rather than egui's built-in theme switch, whose icons the
/// default fonts cannot draw. Follows the system until a fixed theme is picked;
/// egui saves the choice with its other options.
fn draw_theme_choice(ui: &mut egui::Ui) {
    let mut preference = ui.options(|options| options.theme_preference);
    let before = preference;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Right-to-left layout: added in reverse so they read System, Dark, Light.
        for (choice, label, hover) in [
            (ThemePreference::Light, "Light", "Use the light theme"),
            (ThemePreference::Dark, "Dark", "Use the dark theme"),
            (
                ThemePreference::System,
                "System",
                "Follow the operating system's light or dark mode",
            ),
        ] {
            ui.selectable_value(&mut preference, choice, label)
                .on_hover_text(hover);
        }
    });
    if preference != before {
        ui.ctx().set_theme(preference);
    }
}
