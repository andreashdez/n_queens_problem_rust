use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

fn run_command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_n_queens_problem"))
        .args(args)
        .output()
        .expect("failed to run n_queens_problem binary")
}

fn run_success(args: &[&str]) -> Output {
    let output = run_command(args);
    assert!(
        output.status.success(),
        "command failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn stable_summary(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            line.starts_with("Best  =")
                || line.starts_with("Worst =")
                || line.starts_with("Final Population:")
                || line.starts_with("Solved Epoch:")
                || line.starts_with("Board rendering disabled")
        })
        .map(str::to_owned)
        .collect()
}

fn temp_metrics_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "n_queens_problem_{test_name}_{}_{}.csv",
        std::process::id(),
        nanos
    ))
}

#[test]
fn cli_rejects_zero_population() {
    let output = run_command(&["--population", "0", "--no-board"]);

    assert!(!output.status.success());
    assert!(
        output_text(&output).contains("must be greater than 0"),
        "expected validation message, got:\n{}",
        output_text(&output)
    );
}

#[test]
fn cli_rejects_zero_board_size() {
    let output = run_command(&["--size", "0", "--no-board"]);

    assert!(!output.status.success());
    assert!(
        output_text(&output).contains("invalid GA config: board size must be greater than 0"),
        "expected validation message, got:\n{}",
        output_text(&output)
    );
}

#[test]
fn fixed_seed_produces_deterministic_summary() {
    let args = [
        "--size",
        "8",
        "--population",
        "24",
        "--epochs",
        "5",
        "--seed",
        "42",
        "--mutation-rate",
        "0.20",
        "--elite-ratio",
        "0.25",
        "--offspring-ratio",
        "0.50",
        "--no-board",
    ];

    let first = run_success(&args);
    let second = run_success(&args);

    assert_eq!(stable_summary(&first), stable_summary(&second));
}

#[test]
fn quiet_mode_suppresses_log_output() {
    let output = run_success(&[
        "--size",
        "2",
        "--population",
        "8",
        "--epochs",
        "1",
        "--seed",
        "42",
        "--no-board",
        "--quiet",
    ]);
    let text = output_text(&output);

    assert!(text.contains("Best  ="));
    assert!(
        !text.contains(" INFO "),
        "quiet output should not include info logs:\n{text}"
    );
    assert!(
        !text.contains(" WARN "),
        "quiet output should not include warning logs:\n{text}"
    );
}

#[test]
fn log_level_warn_suppresses_info_logs() {
    let output = run_success(&[
        "--size",
        "2",
        "--population",
        "8",
        "--epochs",
        "1",
        "--seed",
        "42",
        "--no-board",
        "--log-level",
        "warn",
    ]);
    let text = output_text(&output);

    assert!(
        text.contains(" WARN "),
        "warn-level output should include warnings:\n{text}"
    );
    assert!(
        !text.contains(" INFO "),
        "warn-level output should not include info logs:\n{text}"
    );
}

#[test]
fn cli_rejects_invalid_log_level() {
    let output = run_command(&["--log-level", "verbose"]);

    assert!(!output.status.success());
    assert!(
        output_text(&output).contains("must be one of: off, error, warn, info, debug, trace"),
        "expected validation message, got:\n{}",
        output_text(&output)
    );
}

#[test]
fn cli_accepts_tournament_selection() {
    let output = run_success(&[
        "--size",
        "8",
        "--population",
        "24",
        "--epochs",
        "3",
        "--seed",
        "42",
        "--selection",
        "tournament",
        "--tournament-size",
        "4",
        "--no-board",
    ]);

    assert!(String::from_utf8_lossy(&output.stdout).contains("Best  ="));
}

#[test]
fn json_mode_emits_machine_readable_summary() {
    let metrics_path = temp_metrics_path("json_metrics");
    let metrics_path_string = metrics_path.display().to_string();
    let output = run_success(&[
        "--size",
        "4",
        "--population",
        "8",
        "--epochs",
        "2",
        "--seed",
        "42",
        "--mutation-rate",
        "0",
        "--elite-ratio",
        "0.25",
        "--offspring-ratio",
        "0",
        "--metrics-csv",
        &metrics_path_string,
        "--json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains(" INFO "),
        "JSON output should not include info logs:\n{stdout}"
    );
    assert!(
        !stdout.contains(" WARN "),
        "JSON output should not include warning logs:\n{stdout}"
    );
    assert!(
        !stdout.contains("Metrics written to"),
        "JSON output should not include human metrics confirmation:\n{stdout}"
    );

    let summary = serde_json::from_str::<Value>(&stdout)
        .unwrap_or_else(|error| panic!("stdout should be valid JSON: {error}\n{stdout}"));

    assert_eq!(summary["seed"], 42);
    assert_eq!(summary["board_size"], 4);
    assert_eq!(summary["target_population"], 8);
    assert_eq!(summary["max_epochs"], 2);
    assert_eq!(summary["final_population"], 8);
    assert_eq!(summary["min_diversity_ratio"], 0.1);
    assert_eq!(summary["selection_strategy"], "roulette");
    assert_eq!(summary["tournament_size"], 3);
    assert!(summary["final_unique_chromosomes"].is_number());
    assert!(summary["final_diversity_ratio"].is_number());
    assert!(summary["last_diversity_replacements"].is_number());
    assert_eq!(summary["metrics_csv"], metrics_path_string);
    assert!(summary["elapsed_ms"].is_number());
    assert!(summary["solved_epoch"].is_null() || summary["solved_epoch"].is_number());
    assert!(summary["best_chromosome"]["positions"].is_array());
    assert!(summary["best_chromosome"]["conflicts"].is_array());
    assert!(summary["best_chromosome"]["conflicts_sum"].is_number());
    assert!(summary["worst_chromosome"]["positions"].is_array());

    fs::remove_file(&metrics_path).expect("temporary metrics CSV should be removable");
}

#[test]
fn metrics_csv_contains_run_configuration_and_epochs() {
    let metrics_path = temp_metrics_path("metrics");
    let metrics_path_string = metrics_path.display().to_string();

    let output = run_success(&[
        "--size",
        "4",
        "--population",
        "8",
        "--epochs",
        "2",
        "--seed",
        "42",
        "--mutation-rate",
        "0",
        "--elite-ratio",
        "0.25",
        "--offspring-ratio",
        "0",
        "--no-board",
        "--metrics-csv",
        &metrics_path_string,
    ]);

    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Metrics written to"),
        "expected metrics confirmation, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let csv = fs::read_to_string(&metrics_path).expect("metrics CSV should be written");
    fs::remove_file(&metrics_path).expect("temporary metrics CSV should be removable");

    let lines = csv.lines().collect::<Vec<_>>();
    assert_eq!(
        lines[0],
        "seed,board_size,target_population,max_epochs,mutation_rate,elite_ratio,offspring_ratio,min_diversity_ratio,selection_strategy,tournament_size,epoch,best_conflicts_sum,population_size,elapsed_ms,average_conflicts_sum,unique_chromosomes,diversity_ratio,epoch_mutation_rate,epoch_elite_ratio,offspring_count,stagnation_epochs,diversity_replacements"
    );
    assert_eq!(lines.len(), 4);
    assert!(lines[1].starts_with("42,4,8,2,0,0.25,0,0.1,roulette,3,0,"));
    assert!(lines[2].starts_with("42,4,8,2,0,0.25,0,0.1,roulette,3,1,"));
    assert!(lines[3].starts_with("42,4,8,2,0,0.25,0,0.1,roulette,3,2,"));
    assert_eq!(lines[1].split(',').count(), 22);
}

#[test]
fn unsolvable_board_sizes_do_not_report_solution() {
    for size in ["2", "3"] {
        let output = run_success(&[
            "--size",
            size,
            "--population",
            "12",
            "--epochs",
            "5",
            "--seed",
            "42",
            "--mutation-rate",
            "0.20",
            "--elite-ratio",
            "0.25",
            "--offspring-ratio",
            "0.50",
            "--no-board",
        ]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let best_line = stdout
            .lines()
            .find(|line| line.starts_with("Best  ="))
            .expect("summary should include best chromosome");

        assert!(
            !stdout.contains("Solved Epoch:"),
            "n={size} should not report a solved epoch\n{stdout}"
        );
        assert!(
            !best_line.contains("conflicts_sum: 0"),
            "n={size} should not have a zero-conflict best chromosome\n{stdout}"
        );
        assert!(
            stdout.contains("no solution found"),
            "n={size} should log that no solution was found\n{stdout}"
        );
    }
}
