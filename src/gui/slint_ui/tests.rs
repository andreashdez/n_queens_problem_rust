//! Drives the Slint GUI through Slint's testing backend, without a window system.

use std::time::Duration;

use i_slint_backend_testing as testing;
use slint::{
    LogicalPosition,
    platform::{Key, PointerEventButton, WindowEvent},
};

use super::*;
use crate::gui::config::GuiConfig;

fn setup() -> (AppWindow, Rc<RefCell<App>>) {
    testing::init_no_event_loop();
    let (ui, app) = start(Storage::None).expect("the testing backend opens windows");
    ui.window().set_size(slint::LogicalSize::new(1400.0, 900.0));
    ui.show().unwrap();
    settle();
    app.borrow_mut().state.config = GuiConfig {
        board_size: 8,
        population_size: 20,
        ..GuiConfig::default()
    };
    (ui, app)
}

/// Runs pending timers and deferred actions, and lays the window out again.
fn settle() {
    for _ in 0..3 {
        testing::mock_elapsed_time(Duration::from_millis(40));
    }
}

/// Polls the solver until the run ends.
fn finish_run(app: &Rc<RefCell<App>>) {
    for _ in 0..10_000 {
        if !app.borrow().state.is_running() {
            settle();
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
        testing::mock_elapsed_time(Duration::from_millis(40));
    }
    panic!("the run did not finish");
}

fn press(ui: &AppWindow, key: impl Into<SharedString> + Clone) {
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: key.clone().into(),
    });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

fn board_origin(ui: &AppWindow) -> LogicalPosition {
    testing::ElementHandle::find_by_element_id(ui, "Board::board-area")
        .next()
        .expect("the board area has an id")
        .absolute_position()
}

fn click(ui: &AppWindow, position: LogicalPosition) {
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

#[test]
fn shortcuts_run_clear_the_selection_and_cancel() {
    let (ui, app) = setup();
    app.borrow_mut().state.config.max_epochs = 1_000_000;
    app.borrow_mut().state.config.board_size = 3;
    app.borrow_mut().state.config.allow_unsolvable = true;
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    press(&ui, Key::Return);
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert!(
        app.borrow().state.is_running(),
        "Ctrl/Cmd+Enter starts a run"
    );
    assert!(ui.global::<TopBarState>().get_running());

    // Wait for the first board, then select a queen on it.
    while app.borrow().state.current_board().is_none() {
        std::thread::sleep(Duration::from_millis(1));
        testing::mock_elapsed_time(Duration::from_millis(40));
    }
    app.borrow_mut().state.selected_queen = Some(0);
    settle();
    assert!(ui.global::<BoardState>().get_has_selection());
    press(&ui, Key::Escape);
    settle();
    assert_eq!(
        app.borrow().state.selected_queen,
        None,
        "Escape clears the selection first"
    );
    assert!(!app.borrow().state.cancel_requested);
    press(&ui, Key::Escape);
    assert!(
        app.borrow().state.cancel_requested,
        "then Escape cancels the run"
    );
    finish_run(&app);
    assert!(
        ui.global::<TopBarState>()
            .get_status()
            .starts_with("Cancelled")
    );
}

#[test]
fn edited_controls_become_the_run_settings() {
    let (ui, app) = setup();
    let controls = ui.global::<ControlsState>();
    controls.set_solver(1);
    controls.set_board_size(70_000);
    controls.set_max_epochs(-5);
    controls.invoke_settings_edited();
    settle();
    {
        let state = &app.borrow().state;
        assert_eq!(state.config.solver, Solver::MinConflicts);
        assert_eq!(state.config.board_size, u16::MAX);
        assert_eq!(state.config.max_epochs, 1);
    }
    assert_eq!(
        controls.get_max_epochs(),
        1,
        "cleaned-up values go back to the controls"
    );
    assert!(controls.get_budget_summary().starts_with("One board"));
    assert_eq!(controls.get_presets().row_count(), GuiPreset::ALL.len() + 1);
    assert_eq!(
        controls.get_preset_index(),
        slint_int(GuiPreset::ALL.len()),
        "custom settings"
    );

    controls.invoke_choose_preset(0);
    settle();
    assert!(GuiPreset::matching(&app.borrow().state.config) == Some(GuiPreset::Demo));
    assert_eq!(controls.get_preset_index(), 0);
    assert_eq!(controls.get_presets().row_count(), GuiPreset::ALL.len());

    controls.set_seed_has_focus(true);
    controls.set_seed_text("12a3".into());
    controls.invoke_seed_edited();
    settle();
    assert_eq!(controls.get_seed_text(), "123");
    assert_eq!(app.borrow().state.config.seed, 123);
}

#[test]
fn a_finished_run_fills_the_board_charts_and_history() {
    let (ui, app) = setup();
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    let board = ui.global::<BoardState>();
    assert!(board.get_has_board());
    assert_eq!(board.get_summary(), "8 queens · 0 attacking pairs");
    assert!(board.get_image().size().width > 100);
    assert_eq!(board.get_queen_labels().row_count(), 8);
    let charts = ui.global::<ChartsState>();
    assert_eq!(charts.get_message(), "");
    assert_eq!(charts.get_charts().row_count(), 2);
    let runs = ui.global::<HistoryState>().get_runs();
    assert_eq!(runs.row_count(), 1);
    assert_eq!(runs.row_data(0).unwrap().title, "Run #1");

    // Click the queen in column 3.
    let (x, y) = {
        let app = app.borrow();
        let (positions, _, _) = app.state.current_board().unwrap();
        let viewport = app.board_viewport;
        let cell = viewport.side / 8.0;
        (
            2.5f32.mul_add(cell, viewport.left),
            (f32::from(positions[2]) + 0.5).mul_add(cell, viewport.top),
        )
    };
    let origin = board_origin(&ui);
    click(&ui, LogicalPosition::new(origin.x + x, origin.y + y));
    settle();
    assert_eq!(app.borrow().state.selected_queen, Some(2));
    assert!(
        board
            .get_selection_title()
            .starts_with("Selected: column 3")
    );

    // Ctrl + scroll zooms the board around the pointer.
    let position = LogicalPosition::new(origin.x + x, origin.y + y);
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    ui.window().dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.0,
        delta_y: 300.0,
    });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    settle();
    assert!(app.borrow().state.board_view.zoom() > 1.5);
    assert_ne!(board.get_zoom_label(), "100%");
    board.invoke_fit();
    assert_eq!(board.get_zoom_label(), "100%");

    // Hovering a chart shows its values.
    charts.invoke_hover(0.5);
    let chart = charts.get_charts().row_data(0).unwrap();
    assert!(chart.hover_x >= 0.0);
    charts.invoke_hover(-1.0);
    assert!(charts.get_charts().row_data(0).unwrap().hover_x < 0.0);
}

#[test]
fn history_marks_compare_view_and_rerun_runs() {
    let (ui, app) = setup();
    let top_bar = ui.global::<TopBarState>();
    for _ in 0..2 {
        top_bar.invoke_run();
        finish_run(&app);
    }
    let history = ui.global::<HistoryState>();
    assert_eq!(top_bar.get_history_count(), 2);
    history.invoke_mark(0, 1);
    history.invoke_mark(1, 2);
    settle();
    let charts = ui.global::<ChartsState>();
    assert_eq!(
        charts.get_view(),
        1,
        "a complete pair brings up the comparison"
    );
    assert!(charts.get_note().starts_with("Same seed"));
    assert_eq!(charts.get_charts().row_count(), 1);
    let runs = history.get_runs();
    assert!(runs.row_data(0).unwrap().marked_b);
    assert!(runs.row_data(1).unwrap().marked_a);

    history.invoke_view(1);
    settle();
    assert!(history.get_viewing_saved());
    assert!(top_bar.get_status().starts_with("Run #1"));
    assert_eq!(
        ui.global::<ControlsState>().get_metrics_title(),
        "Viewed run"
    );
    history.invoke_view_current();
    settle();
    assert!(!history.get_viewing_saved());

    history.invoke_rerun(1);
    assert!(app.borrow().state.is_running());
    finish_run(&app);
    assert_eq!(top_bar.get_history_count(), 3);
}

#[test]
fn choosing_a_theme_recolors_the_app() {
    let (ui, app) = setup();
    let top_bar = ui.global::<TopBarState>();
    top_bar.invoke_choose_theme(2);
    settle();
    assert!(!ui.get_dark());
    assert_eq!(top_bar.get_theme(), 2);
    assert_eq!(
        ui.global::<AppColors>().get_selected_queen(),
        slint_color(Palette::LIGHT.selected_queen)
    );
    top_bar.invoke_choose_theme(1);
    settle();
    assert!(ui.get_dark());
    assert!(app.borrow().dark);
    assert_eq!(
        ui.global::<AppColors>().get_selected_queen(),
        slint_color(Palette::DARK.selected_queen)
    );
}

#[test]
fn notices_show_until_they_expire() {
    let (ui, app) = setup();
    let now = app.borrow().now();
    app.borrow_mut().state.notify("Copied".into(), false, now);
    show_top_bar(&ui, &mut app.borrow_mut());
    let top_bar = ui.global::<TopBarState>();
    assert_eq!(top_bar.get_notice(), "Copied");
    // The notice timer runs on Slint's mocked clock, and the notice expires by the app's.
    app.borrow_mut().clock = web_time::Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap();
    testing::mock_elapsed_time(Duration::from_secs(5));
    assert_eq!(top_bar.get_notice(), "");
}
