mod extractors;
mod history;
mod parser;
mod runner;

use std::path::PathBuf;
use std::time::Duration;

use history::RunSummary;
use runner::Runner;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("logs"))
}

#[tauri::command]
fn start_run(
    app: AppHandle,
    runner: State<Runner>,
    flags: Vec<String>,
    cols: u16,
    rows: u16,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    runner.start(flags, cols, rows, &log_dir(&app)?, on_stream)
}

/// Saved runs, newest first.
#[tauri::command]
fn list_runs(app: AppHandle) -> Result<Vec<RunSummary>, String> {
    Ok(history::list(&log_dir(&app)?))
}

#[tauri::command]
fn replay_run(
    app: AppHandle,
    id: String,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let path = history::log_path(&log_dir(&app)?, &id).ok_or("run not found")?;
    runner::replay(&path, on_stream, None)
}

#[tauri::command]
fn write_input(runner: State<Runner>, data: String) -> Result<(), String> {
    runner.write(&data)
}

#[tauri::command]
fn resize_pty(runner: State<Runner>, cols: u16, rows: u16) -> Result<(), String> {
    runner.resize(cols, rows)
}

#[tauri::command]
fn stop_run(runner: State<Runner>) -> Result<(), String> {
    runner.stop()
}

/// Dev builds only: replays a log from `tests/fixtures` (UI testing without
/// waiting for real updates).
#[tauri::command]
fn replay_fixture(name: String, on_stream: Channel<InvokeResponseBody>) -> Result<(), String> {
    if !cfg!(debug_assertions) || name.contains(['/', '\\']) {
        return Err("not available".into());
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    runner::replay(&path, on_stream, Some(Duration::from_millis(15)))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Runner::default())
        .invoke_handler(tauri::generate_handler![
            start_run,
            write_input,
            resize_pty,
            stop_run,
            list_runs,
            replay_run,
            replay_fixture
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<Runner>().kill();
            }
        });
}
