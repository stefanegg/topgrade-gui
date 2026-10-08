mod parser;
mod runner;

use runner::Runner;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
fn start_run(
    app: AppHandle,
    runner: State<Runner>,
    flags: Vec<String>,
    cols: u16,
    rows: u16,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let log_dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("logs");
    runner.start(flags, cols, rows, &log_dir, on_stream)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Runner::default())
        .invoke_handler(tauri::generate_handler![start_run, write_input, resize_pty, stop_run])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<Runner>().kill();
            }
        });
}
