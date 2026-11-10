//! Drives the Slint GUI through Slint's testing backend, without a window system.

use std::time::Duration;

use i_slint_backend_testing as testing;
use slint::{
    LogicalPosition,
    platform::{Key, PointerEventButton, WindowEvent},
};

use super::*;
use crate::gui::{
    board::{MIN_BOARD_SIDE, board_units},
    config::GuiConfig,
    palette::contrast,
};

/// A window with small, fast run settings.
fn setup() -> (AppWindow, Rc<RefCell<App>>) {
    let (ui, app) = setup_with(Storage::None, 1400.0, 900.0);
    app.borrow_mut().state.config = GuiConfig {
        board_size: 8,
        population_size: 20,
        ..GuiConfig::default()
    };
    (ui, app)
}

fn setup_with(storage: Storage, width: f32, height: f32) -> (AppWindow, Rc<RefCell<App>>) {
    thread_local! {
        static INITIALIZED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    // Each test thread sets up the testing backend once, however many windows it opens.
    if !INITIALIZED.replace(true) {
        testing::init_no_event_loop();
    }
    let (ui, app) = start(storage).expect("the testing backend opens windows");
    ui.window().set_size(slint::LogicalSize::new(width, height));
    ui.show().unwrap();
    settle();
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

/// The first element with this accessible label, such as a button's text.
fn element(ui: &AppWindow, label: &str) -> testing::ElementHandle {
    testing::ElementHandle::find_by_accessible_label(ui, label)
        .next()
        .unwrap_or_else(|| panic!("no element labelled {label:?}"))
}

fn has_element(ui: &AppWindow, label: &str) -> bool {
    testing::ElementHandle::find_by_accessible_label(ui, label)
        .next()
        .is_some()
}

fn center(element: &testing::ElementHandle) -> LogicalPosition {
    let (position, size) = (element.absolute_position(), element.size());
    LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    )
}

/// Clicks the middle of the element labelled `label` with the pointer.
fn click_on(ui: &AppWindow, label: &str) {
    click(ui, center(&element(ui, label)));
    settle();
}

fn type_text(ui: &AppWindow, text: &str) {
    for character in text.chars() {
        press(ui, SharedString::from(character.to_string()));
    }
    settle();
}

fn scroll(ui: &AppWindow, position: LogicalPosition, delta_x: f32, delta_y: f32, control: bool) {
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    if control {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
    }
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x,
        delta_y,
    });
    if control {
        window.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
    }
    settle();
}

fn drag(ui: &AppWindow, from: LogicalPosition, to: LogicalPosition) {
    let window = ui.window();
    let button = PointerEventButton::Left;
    window.dispatch_event(WindowEvent::PointerMoved { position: from });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button,
    });
    for step in 1..=4u8 {
        let fraction = f32::from(step) / 4.0;
        let position = LogicalPosition::new(
            (to.x - from.x).mul_add(fraction, from.x),
            (to.y - from.y).mul_add(fraction, from.y),
        );
        window.dispatch_event(WindowEvent::PointerMoved { position });
    }
    window.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button,
    });
    settle();
}

/// A two-finger pinch that scales by `factor`, as a trackpad reports it.
fn pinch(ui: &AppWindow, position: LogicalPosition, factor: f32) {
    use i_slint_core::{
        input::{BackendMouseEvent, TouchPhase},
        lengths::LogicalPoint,
    };

    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    let position = LogicalPoint::new(position.x, position.y);
    for (delta, phase) in [
        (0.0, TouchPhase::Started),
        (factor - 1.0, TouchPhase::Moved),
        (0.0, TouchPhase::Ended),
    ] {
        window.dispatch_event(WindowEvent::internal(BackendMouseEvent::PinchGesture {
            position,
            delta,
            phase,
        }));
    }
    settle();
}

/// Where queen `column` sits on screen.
fn queen_position(ui: &AppWindow, app: &Rc<RefCell<App>>, column: usize) -> LogicalPosition {
    let app = app.borrow();
    let (positions, _, _) = app.state.current_board().expect("a board is on display");
    let board = app.state.board_view.board(app.board_viewport);
    let cell = board.side / board_units(positions.len());
    let origin = board_origin(ui);
    LogicalPosition::new(
        origin.x + (board_units(column) + 0.5).mul_add(cell, board.left),
        origin.y + (f32::from(positions[column]) + 0.5).mul_add(cell, board.top),
    )
}

fn plot(ui: &AppWindow) -> testing::ElementHandle {
    testing::ElementHandle::find_by_element_id(ui, "Chart::plot")
        .next()
        .expect("a chart is on display")
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

#[test]
fn the_run_buttons_start_and_cancel_runs() {
    let (ui, app) = setup();
    {
        let config = &mut app.borrow_mut().state.config;
        config.board_size = 3;
        config.max_epochs = 1_000_000;
        config.allow_unsolvable = true;
    }
    click_on(&ui, "Run solver");
    assert!(app.borrow().state.is_running());
    // Live progress reaches the top bar and the metrics panel while the run goes on.
    while app.borrow().state.latest_snapshot.is_none() {
        std::thread::sleep(Duration::from_millis(1));
        testing::mock_elapsed_time(Duration::from_millis(40));
    }
    let top_bar = ui.global::<TopBarState>();
    assert!(top_bar.get_progress_text().starts_with("Epoch "));
    assert!(top_bar.get_status().starts_with("Best board so far"));
    assert!(ui.global::<ControlsState>().get_has_metrics());
    assert!(
        ui.global::<ControlsState>().get_running(),
        "settings are locked"
    );

    // The run button turns into the cancel button under the pointer. Checked
    // straight after the click: the solver stops within a few milliseconds.
    click(&ui, center(&element(&ui, "Cancel run")));
    assert!(app.borrow().state.cancel_requested);
    assert_eq!(
        element(&ui, "Cancelling…").accessible_enabled(),
        Some(false)
    );
    finish_run(&app);
    assert!(top_bar.get_status().starts_with("Cancelled"));
    assert!(has_element(&ui, "Run solver"));
}

#[test]
fn solver_and_advanced_settings_follow_clicks() {
    let (ui, app) = setup();
    assert!(
        !has_element(&ui, "Population"),
        "advanced settings start closed"
    );
    click_on(&ui, "Advanced settings");
    assert_eq!(
        element(&ui, "Advanced settings").accessible_expanded(),
        Some(true)
    );
    assert!(has_element(&ui, "Population"));

    click_on(&ui, "Tournament");
    click_on(&ui, "Roulette");
    assert_eq!(
        app.borrow().state.config.selection_strategy,
        SelectionStrategy::Roulette
    );
    assert_eq!(element(&ui, "Roulette").accessible_checked(), Some(true));
    assert_eq!(element(&ui, "Tournament").accessible_checked(), Some(false));

    click_on(&ui, "Min-conflicts");
    assert_eq!(app.borrow().state.config.solver, Solver::MinConflicts);
    assert!(has_element(&ui, "Max passes"));
    assert!(
        !has_element(&ui, "Population"),
        "min-conflicts has no population"
    );
    click_on(&ui, "Genetic algorithm");
    assert_eq!(app.borrow().state.config.solver, Solver::Ga);
}

#[test]
fn typing_a_seed_keeps_only_digits() {
    let (ui, app) = setup();
    let field = testing::ElementHandle::find_by_element_type_name(&ui, "LineEdit")
        .next()
        .expect("the seed field");
    click(&ui, center(&field));
    settle();
    assert!(ui.global::<ControlsState>().get_seed_has_focus());
    for _ in 0..4 {
        press(&ui, Key::Backspace);
    }
    type_text(&ui, "7a1");
    assert_eq!(ui.global::<ControlsState>().get_seed_text(), "71");
    assert_eq!(app.borrow().state.config.seed, 71);

    // A seed set elsewhere shows up once the field loses focus.
    app.borrow_mut().state.config.seed = 5;
    click_on(&ui, "Set up a run");
    assert!(!ui.global::<ControlsState>().get_seed_has_focus());
    assert_eq!(ui.global::<ControlsState>().get_seed_text(), "5");
}

#[test]
fn the_board_follows_pointer_gestures() {
    let (ui, app) = setup();
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    let board = ui.global::<BoardState>();

    // Hovering a queen describes it.
    let queen = queen_position(&ui, &app, 0);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: queen });
    settle();
    assert!(board.get_hover_text().starts_with("Column 1, row "));
    assert!(board.get_hovering_queen());

    // Clicking a queen twice selects it and then clears the selection.
    click(&ui, queen);
    settle();
    assert_eq!(app.borrow().state.selected_queen, Some(0));
    click(&ui, queen);
    settle();
    assert_eq!(app.borrow().state.selected_queen, None);

    // The zoom buttons, then a drag and a plain scroll pan the zoomed board.
    click_on(&ui, "+");
    assert_eq!(board.get_zoom_label(), "150%");
    click_on(&ui, "+");
    let before = app.borrow().state.board_view;
    let middle = {
        let viewport = app.borrow().board_viewport;
        let origin = board_origin(&ui);
        LogicalPosition::new(
            origin.x + viewport.left + viewport.side / 2.0,
            origin.y + viewport.top + viewport.side / 2.0,
        )
    };
    drag(
        &ui,
        middle,
        LogicalPosition::new(middle.x + 60.0, middle.y + 40.0),
    );
    let dragged = app.borrow().state.board_view;
    assert_ne!(dragged, before, "dragging pans");
    assert_eq!(
        app.borrow().state.selected_queen,
        None,
        "a drag is not a click"
    );
    scroll(&ui, middle, 0.0, -50.0, false);
    assert_ne!(app.borrow().state.board_view, dragged, "scrolling pans");
    click_on(&ui, "Fit");
    assert_eq!(board.get_zoom_label(), "100%");
    assert_eq!(element(&ui, "−").accessible_enabled(), Some(false));
    pinch(&ui, middle, 2.0);
    assert_eq!(board.get_zoom_label(), "200%", "pinching zooms");
}

#[test]
fn charts_follow_pointer_gestures_and_the_legend() {
    let (ui, app) = setup();
    {
        let config = &mut app.borrow_mut().state.config;
        *config = GuiPreset::Default.config(42);
    }
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    let charts = ui.global::<ChartsState>();
    assert_eq!(charts.get_charts().row_count(), 2);

    let plot = plot(&ui);
    let middle = center(&plot);
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: middle });
    settle();
    let hovered = charts.get_charts().row_data(0).unwrap();
    assert!(hovered.hover_x > 0.0);
    assert!(hovered.hover_text.contains("Best board · epoch "));
    // Charts stacked under each other share the hover line.
    assert!(charts.get_charts().row_data(1).unwrap().hover_x > 0.0);

    let ticks = |index| {
        charts
            .get_charts()
            .row_data(index)
            .unwrap()
            .x_ticks
            .row_count()
    };
    let first_tick = || {
        charts
            .get_charts()
            .row_data(0)
            .unwrap()
            .x_ticks
            .row_data(0)
            .unwrap()
            .text
    };
    let fitted = first_tick();
    scroll(&ui, middle, 0.0, 400.0, true);
    assert!(app.borrow().charts.zoomed(), "Ctrl + scroll zooms");
    assert!(ticks(0) >= 2);
    drag(
        &ui,
        middle,
        LogicalPosition::new(middle.x - 150.0, middle.y),
    );
    assert_ne!(first_tick(), fitted, "dragging pans");
    let window = ui.window();
    for _ in 0..2 {
        window.dispatch_event(WindowEvent::PointerPressed {
            position: middle,
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position: middle,
            button: PointerEventButton::Left,
        });
    }
    settle();
    assert!(
        !app.borrow().charts.zoomed(),
        "a double click resets the zoom"
    );
    pinch(&ui, middle, 2.0);
    assert!(app.borrow().charts.zoomed(), "pinching zooms");

    let lines = || charts.get_charts().row_data(0).unwrap().lines.row_count();
    let shown = lines();
    click_on(&ui, "Population average");
    let legend = charts.get_charts().row_data(0).unwrap().legend;
    assert!(
        legend
            .iter()
            .any(|entry| entry.label == "Population average" && entry.hidden)
    );
    assert_eq!(lines(), shown - 1, "the hidden line is not drawn");
    click_on(&ui, "Population average");
    assert_eq!(lines(), shown);
}

#[test]
fn history_copies_commands_resizes_and_hides() {
    let (ui, app) = setup();
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    click_on(&ui, "Copy CLI command");
    let command = app.borrow().state.cli_command(1).unwrap();
    assert_eq!(app.borrow().copied, [command]);
    assert_eq!(
        ui.global::<TopBarState>().get_notice(),
        "Copied command for run #1"
    );

    // Dragging the panel's left edge widens it, up to a limit.
    let edge = || {
        center(
            &testing::ElementHandle::find_by_element_id(&ui, "History::edge")
                .next()
                .expect("the history panel's edge has an id"),
        )
    };
    let start = edge();
    drag(&ui, start, LogicalPosition::new(start.x - 80.0, start.y));
    assert!((ui.get_history_width() - 400.0).abs() < 0.5);
    let start = edge();
    drag(&ui, start, LogicalPosition::new(start.x - 2000.0, start.y));
    assert!(
        (ui.get_history_width() - 590.0).abs() < 0.5,
        "1400px window"
    );

    click_on(&ui, "Run history (1)");
    assert!(!has_element(&ui, "Copy CLI command"), "the panel is hidden");
    assert!(!app.borrow().state.show_history);
    click_on(&ui, "Run history (1)");
    assert!(has_element(&ui, "Copy CLI command"));
}

/// Slint's royalty-free license asks for its About widget in an About dialog.
#[test]
fn about_shows_the_slint_notice() {
    let (ui, _app) = setup();
    assert!(
        testing::ElementHandle::find_by_element_type_name(&ui, "AboutSlint")
            .next()
            .is_none()
    );
    click_on(&ui, "About");
    assert!(
        testing::ElementHandle::find_by_element_type_name(&ui, "AboutSlint")
            .next()
            .is_some()
    );
}

#[test]
fn min_conflicts_runs_fill_every_panel() {
    let (ui, app) = setup();
    app.borrow_mut().state.config = GuiPreset::MinConflicts.config(3);
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    assert!(
        ui.global::<TopBarState>()
            .get_status()
            .starts_with("Solved at pass ")
    );
    let controls = ui.global::<ControlsState>();
    assert_eq!(controls.get_metrics().row_data(0).unwrap().label, "Pass");
    let charts = ui.global::<ChartsState>().get_charts();
    assert_eq!(charts.row_count(), 1);
    assert_eq!(charts.row_data(0).unwrap().x_label, "Pass");
    let board = ui.global::<BoardState>();
    assert_eq!(board.get_summary(), "1000 queens · 0 attacking pairs");
    // A thousand queens are too small for attacker counts.
    assert_eq!(board.get_queen_labels().row_count(), 0);
}

#[test]
fn settings_survive_a_restart() {
    let path = std::env::temp_dir().join(format!(
        "n-queens-settings-test-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let (ui, app) = setup_with(Storage::File(path.clone()), 1400.0, 900.0);
        ui.global::<ControlsState>().invoke_choose_preset(3);
        click_on(&ui, "Log scale");
        ui.global::<TopBarState>().invoke_choose_theme(2);
        settle();
        assert!(!app.borrow().state.log_conflicts);
        ui.window().set_size(slint::LogicalSize::new(1300.0, 850.0));
        ui.set_chart_height(350.0);
        ui.set_history_width(300.0);
        settle();
        // Sizes are saved once resizing pauses.
        testing::mock_elapsed_time(LAYOUT_SAVE_DELAY);
    }
    let saved = std::fs::read_to_string(&path).expect("settings were written");
    assert!(saved.contains(r#""theme":"light""#), "{saved}");
    {
        let (ui, _app) = start(Storage::File(path.clone())).unwrap();
        let window = ui.window();
        let size = window.size().to_logical(window.scale_factor());
        assert_eq!((size.width, size.height), (1300.0, 850.0), "{saved}");
    }
    {
        let (ui, app) = setup_with(Storage::File(path.clone()), 1400.0, 900.0);
        assert!(GuiPreset::matching(&app.borrow().state.config) == Some(GuiPreset::Classic));
        assert!(!ui.global::<ChartsState>().get_log_scale());
        assert_eq!(ui.global::<TopBarState>().get_theme(), 2);
        assert!(!ui.get_dark());
        assert!((ui.get_chart_height() - 350.0).abs() < f32::EPSILON);
        assert!((ui.get_history_width() - 300.0).abs() < f32::EPSILON);
    }
    // A damaged file falls back to the defaults.
    std::fs::write(&path, "{not json").unwrap();
    let (_ui, app) = setup_with(Storage::File(path.clone()), 1400.0, 900.0);
    assert!(GuiPreset::matching(&app.borrow().state.config) == Some(GuiPreset::Default));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn the_smallest_window_still_fits_every_panel() {
    let (ui, app) = setup_with(Storage::None, 1000.0, 680.0);
    ui.global::<TopBarState>().invoke_run();
    finish_run(&app);
    let viewport = app.borrow().board_viewport;
    assert!(
        viewport.side >= MIN_BOARD_SIDE,
        "board side {}",
        viewport.side
    );
    let area = testing::ElementHandle::find_by_element_id(&ui, "Board::board-area")
        .next()
        .unwrap();
    let history = testing::ElementHandle::find_by_element_type_name(&ui, "History")
        .next()
        .unwrap();
    let controls = testing::ElementHandle::find_by_element_type_name(&ui, "Controls")
        .next()
        .unwrap();
    let right =
        |element: &testing::ElementHandle| element.absolute_position().x + element.size().width;
    let bottom =
        |element: &testing::ElementHandle| element.absolute_position().y + element.size().height;
    assert!(
        area.absolute_position().x >= right(&controls),
        "the board starts right of the settings"
    );
    assert!(
        right(&area) <= history.absolute_position().x,
        "and ends left of run history"
    );
    assert!(right(&history) <= 1000.0, "run history fits in the window");
    assert!(
        bottom(&controls) <= 680.0,
        "the panels fit the window's height"
    );
    let plot = plot(&ui);
    assert!(plot.size().width > 200.0 && plot.size().height > 40.0);
}

#[test]
fn palette_text_is_readable_on_the_slint_panels() {
    let (ui, _app) = setup();
    for (theme, palette) in [(1, &Palette::DARK), (2, &Palette::LIGHT)] {
        ui.global::<TopBarState>().invoke_choose_theme(theme);
        settle();
        for background in [ui.get_panel_background(), ui.get_bar_background()] {
            let panel = Rgba::rgb(background.red(), background.green(), background.blue());
            for (text, label) in [
                (palette.success, "success"),
                (palette.warning, "warning"),
                (palette.error_text, "error"),
                (palette.selected_queen, "selected queen"),
                (palette.coordinate, "coordinates"),
            ] {
                let ratio = contrast(text, panel);
                assert!(ratio >= 4.5, "theme {theme}, {label} text: {ratio:.2}");
            }
        }
    }
}
