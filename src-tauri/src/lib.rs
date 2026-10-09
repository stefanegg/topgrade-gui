mod extractors;
mod history;
mod parser;
mod runner;
mod settings;
mod tray;

use std::time::Duration;

use history::RunSummary;
use runner::Runner;
use settings::Settings;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tray::AppState;

/// Passed by the login item; the app starts hidden anyway, but this keeps
/// login launches identifiable.
const HIDDEN_ARG: &str = "--hidden";

#[tauri::command]
fn start_run(
    app: AppHandle,
    runner: State<Runner>,
    state: State<AppState>,
    flags: Vec<String>,
    cols: u16,
    rows: u16,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    runner.start(flags, cols, rows, &state.log_dir(), on_stream, move || {
        tray::refresh(&app)
    })
}

/// Saved runs, newest first.
#[tauri::command]
fn list_runs(state: State<AppState>) -> Vec<RunSummary> {
    history::list(&state.log_dir())
}

#[tauri::command]
fn replay_run(
    state: State<AppState>,
    id: String,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let path = history::log_path(&state.log_dir(), &id).ok_or("run not found")?;
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

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    settings::save(&state.data_dir, &settings).map_err(|e| e.to_string())?;
    let autostart_changed = {
        let mut current = state.settings.lock().unwrap();
        let changed = current.autostart != settings.autostart;
        *current = settings.clone();
        changed
    };
    if autostart_changed {
        apply_autostart(&app, settings.autostart)?;
    }
    Ok(())
}

/// Registers or removes the login item. Skipped in dev builds, which would
/// register the debug binary.
fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| e.to_string())
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

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let (settings, existed) = settings::load(&data_dir);
    if !existed {
        // First launch: persist defaults and register the login item.
        settings::save(&data_dir, &settings)?;
        let _ = apply_autostart(app, settings.autostart);
    }
    app.manage(AppState::new(data_dir, settings));
    tray::setup(app)?;

    // A tray app starts hidden (window is `visible: false` in
    // tauri.conf.json); only the very first launch shows the window so a new
    // install doesn't look like nothing happened.
    if existed {
        tray::hide_window(app);
    } else {
        tray::show_window(app);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .macos_launcher(MacosLauncher::LaunchAgent)
                .arg(HIDDEN_ARG)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Runner::default())
        .setup(|app| setup(app.handle()))
        // Closing the window keeps the app (and reminders) running in the tray.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                tray::hide_window(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_run,
            write_input,
            resize_pty,
            stop_run,
            list_runs,
            replay_run,
            get_settings,
            set_settings,
            replay_fixture
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => app.state::<Runner>().kill(),
            // macOS: clicking the Dock icon while the window is hidden.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_window(app),
            _ => {}
        });
}
