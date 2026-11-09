//! Draws the GUI with egui: window setup, the panel layout and keyboard shortcuts.
//!
//! Each panel is drawn by its own module: [`top_bar`] for run actions and status,
//! [`controls`] for the left settings panel, [`board`] for the central board,
//! [`charts`] for the bottom chart panel and [`history`] for the right run-history panel.
//! Everything they show and do comes from [`AppState`].

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers};

use super::{
    board::{Point, Square},
    palette::{Palette, Rgba},
    state::{AppState, ChartView},
    worker::POLL_INTERVAL,
};

mod board;
mod charts;
mod controls;
mod history;
mod top_bar;

/// Opens the native solver interface.
///
/// # Errors
/// Returns an eframe error if the window or graphics backend cannot initialize or run.
#[cfg(not(target_arch = "wasm32"))]
pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("N-Queens Solver")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1000.0, 680.0])
            .with_icon(app_icon()),
        ..Default::default()
    };

    // The app name locates saved settings, so it stays as it was when the GA was the only solver.
    eframe::run_native(
        "N-Queens Genetic Solver",
        native_options,
        Box::new(|cc| Ok(Box::new(new_app(cc)))),
    )
}

/// The window and Dock icon, rendered from `assets/app-icon.svg`.
///
/// Inside a macOS `.app`, the bundle's Icon Composer icon applies instead, so
/// this returns the empty icon, which eframe leaves unset.
#[cfg(not(target_arch = "wasm32"))]
fn app_icon() -> egui::IconData {
    // The executable sits at `<name>.app/Contents/MacOS/<binary>`.
    let in_app_bundle = cfg!(target_os = "macos")
        && std::env::current_exe().is_ok_and(|exe| {
            exe.ancestors()
                .nth(3)
                .and_then(std::path::Path::extension)
                .is_some_and(|extension| extension == "app")
        });
    if in_app_bundle {
        return egui::IconData::default();
    }
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/app-icon.png"))
        .expect("the bundled app icon is a valid PNG")
}

/// Starts the solver interface in the page's `<canvas>` with id `canvas_id`.
///
/// The element with id `status_id`, if the page has one, is removed once the
/// interface starts, or shows the error if it cannot.
///
/// # Panics
/// Panics if the page has no canvas with that id.
#[cfg(target_arch = "wasm32")]
pub fn run_web(canvas_id: &str, status_id: &str) {
    use wasm_bindgen::JsCast as _;

    // Fails only if a logger is already installed, which leaves logging working.
    let _ = eframe::WebLogger::init(log::LevelFilter::Warn);
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("runs in a browser page");
    let status = document.get_element_by_id(status_id);
    let canvas = document
        .get_element_by_id(canvas_id)
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .unwrap_or_else(|| panic!("the page has no <canvas id=\"{canvas_id}\">"));

    wasm_bindgen_futures::spawn_local(async move {
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(new_app(cc)))),
            )
            .await;
        match (started, status) {
            (Ok(()), Some(status)) => status.remove(),
            (Ok(()), None) => {}
            (Err(error), status) => {
                log::error!("could not start the GUI: {error:?}");
                if let Some(status) = status {
                    // Usually a browser without WebGL.
                    status
                        .set_text_content(Some(&format!("Could not start the solver: {error:?}")));
                }
            }
        }
    });
}

fn new_app(cc: &eframe::CreationContext<'_>) -> AppState {
    let saved = cc
        .storage
        .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
        .unwrap_or_default();
    AppState::with_settings(saved)
}

const RUN_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
const CANCEL_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Escape);

/// Room for the tab row and two charts at their minimum plot height.
const CHART_PANEL_MIN_HEIGHT: f32 = 300.0;

impl From<Rgba> for Color32 {
    fn from(color: Rgba) -> Self {
        Self::from_rgba_unmultiplied_const(color.r, color.g, color.b, color.a)
    }
}

/// The palette for the theme egui is currently drawing with.
fn palette(ctx: &egui::Context) -> &'static Palette {
    Palette::of(ctx.global_style().visuals.dark_mode)
}

const fn point(pos: egui::Pos2) -> Point {
    Point::new(pos.x, pos.y)
}

const fn pos(point: Point) -> egui::Pos2 {
    egui::Pos2::new(point.x, point.y)
}

/// egui draws boards in square rects.
const fn square(rect: egui::Rect) -> Square {
    Square {
        left: rect.min.x,
        top: rect.min.y,
        side: rect.max.x - rect.min.x,
    }
}

fn rect(square: Square) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::Pos2::new(square.left, square.top),
        egui::Vec2::splat(square.side),
    )
}

/// Seconds on egui's clock, which notices expire by.
fn now(ctx: &egui::Context) -> f64 {
    ctx.input(|input| input.time)
}

impl AppState {
    /// Runs after the panels so a focused board can claim Escape for its selection first.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.running.is_none() {
            if ctx.input_mut(|input| input.consume_shortcut(&RUN_SHORTCUT)) {
                self.start_run();
            }
        } else if ctx.input_mut(|input| input.consume_shortcut(&CANCEL_SHORTCUT)) {
            self.cancel_run();
        }
    }

    fn draw_main_panel(&mut self, ui: &mut egui::Ui) {
        let mut selected_queen = self.selected_queen;
        let mut board_view = self.board_view;
        let board = self.current_board();
        ui.vertical_centered(|ui| {
            if let Some((positions, conflicts, conflicts_sum)) = board {
                board::draw_board(
                    ui,
                    positions,
                    conflicts,
                    conflicts_sum,
                    &mut selected_queen,
                    &mut board_view,
                );
            } else {
                ui.heading("Board");
                board::draw_empty_board(ui);
            }
        });

        self.selected_queen = selected_queen;
        self.board_view = board_view;
    }

    fn draw_chart_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.chart_view, ChartView::Metrics, "Run metrics")
                .on_hover_text("Attacking pairs and other metrics of the run on the board");
            ui.selectable_value(&mut self.chart_view, ChartView::Comparison, "Compare A/B")
                .on_hover_text("Best-board attacking pairs of the two runs marked A and B in run history");
            ui.separator();
            ui.checkbox(&mut self.log_conflicts, "Log scale")
                .on_hover_text("Plot attacking pairs on a log scale so small best-board counts stay visible next to larger counts.");
            ui.separator();
            ui.weak("Chart tips").on_hover_text(
                "Drag to pan · Ctrl/Cmd + scroll to zoom · double-click to reset · click legend entries to hide them",
            );
        });
        match self.chart_view {
            ChartView::Metrics => charts::draw_metric_charts(
                ui,
                self.chart_history(),
                (self.archive.viewed, self.chart_generation),
                self.log_conflicts,
            ),
            ChartView::Comparison => charts::draw_comparison(ui, &self.archive, self.log_conflicts),
        }
    }
}

impl eframe::App for AppState {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_worker();
        if self.is_running() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            self.draw_top_bar(ui, &ctx);
            ui.add_space(4.0);
        });

        egui::Panel::left("controls")
            .resizable(false)
            .default_size(315.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_controls(ui);
                });
            });

        let mut show_history = self.show_history;
        egui::Panel::right("run_history")
            .resizable(true)
            .default_size(300.0)
            .size_range(240.0..=520.0)
            .show_collapsible(ui, &mut show_history, |ui| {
                // Without auto-shrink the panel keeps its width instead of hugging the content.
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.draw_run_history(ui, &ctx));
            });
        self.show_history = show_history;

        egui::Panel::bottom("charts")
            .resizable(true)
            .default_size(400.0)
            .size_range(CHART_PANEL_MIN_HEIGHT..=900.0)
            .show(ui, |ui| {
                self.draw_chart_panel(ui);
                // Resizable panels shrink to their content, so claim the rest of the height.
                ui.allocate_space(ui.available_size());
            });

        egui::CentralPanel::default().show(ui, |ui| self.draw_main_panel(ui));
        self.handle_shortcuts(&ctx);
        // A run started this frame is polled from the next one.
        if self.is_running() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.saved_settings());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{config, palette::contrast, state::SavedSettings};

    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn settings_survive_a_restart_through_eframe_storage() {
        let mut state = AppState::default();
        state.config = config::GuiPreset::Classic.config(987);
        state.log_conflicts = false;
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut state, &mut storage);

        let saved: SavedSettings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
        let restored = AppState::with_settings(saved);
        assert!(restored.config == state.config);
        assert!(!restored.log_conflicts);

        // Older saves are RON written by eframe.
        eframe::Storage::set_string(
            &mut storage,
            eframe::APP_KEY,
            "(config: (board_size: 0, selection_strategy: \"roulette\"))".to_owned(),
        );
        let saved: SavedSettings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
        assert_eq!(AppState::with_settings(saved).config.board_size, 1);
    }

    fn frame_at(ctx: &egui::Context, time: f64, events: Vec<egui::Event>, state: &mut AppState) {
        let input = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            state.draw_top_bar(ui, &ui.ctx().clone());
            state.handle_shortcuts(ui.ctx());
        })
        .drop_without_applying_deltas();
    }

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn keyboard_shortcuts_start_and_cancel_runs() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        state.config = config::GuiConfig {
            board_size: 3,
            population_size: 4,
            max_epochs: 100_000,
            allow_unsolvable: true,
            ..Default::default()
        };
        frame_at(
            &ctx,
            0.0,
            vec![key(Key::Escape, Modifiers::NONE)],
            &mut state,
        );
        assert!(state.running.is_none());
        frame_at(
            &ctx,
            0.1,
            vec![key(Key::Enter, Modifiers::COMMAND)],
            &mut state,
        );
        assert!(state.running.is_some());
        frame_at(
            &ctx,
            0.2,
            vec![key(Key::Escape, Modifiers::NONE)],
            &mut state,
        );
        assert!(state.cancel_requested);
    }

    #[test]
    fn notices_show_until_they_expire() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        frame_at(&ctx, 0.0, Vec::new(), &mut state);
        state.notify("Copied".into(), false, now(&ctx));
        frame_at(&ctx, 1.0, Vec::new(), &mut state);
        assert!(state.notice.is_some());
        frame_at(&ctx, 60.0, Vec::new(), &mut state);
        assert!(state.notice.is_none());
    }

    #[test]
    fn every_panel_draws_a_finished_run_without_a_window() {
        for solver in [crate::Solver::Ga, crate::Solver::MinConflicts] {
            let mut state = AppState::default();
            state.config = config::GuiConfig {
                solver,
                board_size: 8,
                population_size: 20,
                ..Default::default()
            };
            state.start_run();
            while state.is_running() {
                state.drain_worker();
            }
            assert!(state.result.is_some());
            state.selected_queen = Some(0);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let ctx = ui.ctx().clone();
                state.draw_top_bar(ui, &ctx);
                state.draw_controls(ui);
                state.draw_chart_panel(ui);
                state.draw_main_panel(ui);
            });
            assert_ne!(output.shapes.len(), 0);
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn text_colors_are_readable_on_egui_panels() {
        for (palette, visuals) in [
            (&Palette::DARK, egui::Visuals::dark()),
            (&Palette::LIGHT, egui::Visuals::light()),
        ] {
            let name = if visuals.dark_mode { "dark" } else { "light" };
            let fill = visuals.panel_fill;
            let panel = Rgba::rgb(fill.r(), fill.g(), fill.b());
            for (text, label) in [
                (palette.success, "success"),
                (palette.warning, "warning"),
                (palette.error_text, "error"),
                (palette.selected_queen, "selected queen"),
                (palette.coordinate, "coordinates"),
            ] {
                let ratio = contrast(text, panel);
                assert!(ratio >= 4.5, "{name} {label} text: {ratio:.2}");
            }
            let ctx = egui::Context::default();
            ctx.set_theme(if visuals.dark_mode {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            assert_eq!(super::palette(&ctx).queen_label, palette.queen_label);
        }
    }

    #[test]
    fn palette_colors_convert_exactly() {
        assert_eq!(
            Color32::from(Rgba::translucent(120, 210, 220, 45)),
            Color32::from_rgba_unmultiplied_const(120, 210, 220, 45)
        );
        assert_eq!(Color32::from(Rgba::GRAY), Color32::GRAY);
        assert_eq!(Color32::from(Rgba::WHITE), Color32::WHITE);
    }
}
