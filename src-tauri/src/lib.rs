mod autostart;
mod checks;
mod extractors;
mod history;
mod parser;
mod runner;
mod settings;
mod sources;
mod tray;
mod updater;
mod window_state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use history::RunSummary;
use runner::Runner;
use settings::Settings;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tray::AppState;

/// Passed by the Linux autostart entry; the app starts hidden anyway, but
/// this keeps login launches identifiable.
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
    let tray_app = app.clone();
    runner.start(flags, cols, rows, &state.log_dir(), on_stream, move || {
        tray::refresh(&app)
    })?;
    tray::animate_while_running(&tray_app);
    Ok(())
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
fn set_waiting_input(app: AppHandle, waiting: bool, password: bool) {
    tray::set_waiting(&app, waiting, password);
}

#[tauri::command]
fn resize_pty(runner: State<Runner>, cols: u16, rows: u16) -> Result<(), String> {
    runner.resize(cols, rows)
}

#[tauri::command]
fn stop_run(runner: State<Runner>) -> Result<(), String> {
    runner.stop()
}

/// What topgrade updates on this machine, from the last background dry run.
#[tauri::command]
fn get_sources(state: State<AppState>) -> Option<sources::Sources> {
    sources::load(&state.data_dir)
}

/// Runs a background `topgrade --dry-run` to (re)detect the sources and
/// caches the result. Only one at a time.
#[tauri::command]
async fn detect_sources(app: AppHandle) -> Result<sources::Sources, String> {
    static DETECTING: AtomicBool = AtomicBool::new(false);
    if DETECTING.swap(true, Ordering::SeqCst) {
        return Err("already checking".into());
    }
    let result = tauri::async_runtime::spawn_blocking(sources::detect)
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    DETECTING.store(false, Ordering::SeqCst);
    let detected = result?;
    let _ = sources::save(&app.state::<AppState>().data_dir, &detected);
    Ok(detected)
}

/// Waiting updates per step, from the last check.
#[tauri::command]
fn get_checks(state: State<AppState>) -> Option<checks::Checks> {
    checks::load(&state.data_dir)
}

/// Looks for waiting updates in `steps` (those with a check) and caches the
/// result. Only one at a time.
#[tauri::command]
async fn run_checks(app: AppHandle, steps: Vec<String>) -> Result<checks::Checks, String> {
    static CHECKING: AtomicBool = AtomicBool::new(false);
    if CHECKING.swap(true, Ordering::SeqCst) {
        return Err("already checking".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || checks::run(&steps)).await;
    CHECKING.store(false, Ordering::SeqCst);
    let found = result.map_err(|e| e.to_string())?;
    let _ = checks::save(&app.state::<AppState>().data_dir, &found);
    Ok(found)
}

/// Semver derived from git tags at build time (see build.rs).
#[tauri::command]
fn app_version() -> &'static str {
    env!("APP_VERSION")
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
        autostart::apply(&app, settings.autostart)?;
    }
    Ok(())
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
        settings::save(&data_dir, &settings)?;
    }
    // Keep the login item in sync with the setting (also registers it on the
    // first release launch, and re-points it if the app moved).
    let _ = autostart::apply(app, settings.autostart);
    app.manage(AppState::new(data_dir, settings));
    app.manage(updater::UpdateState::default());
    tray::setup(app)?;
    updater::start_background_checks(app.clone());

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
    // Wayland doesn't let clients read or set their window position, so
    // remembering it needs XWayland. Respect an explicit GDK_BACKEND.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    // Must be the first plugin. A second launch just brings up the running
    // window (unless it's the hidden login-item start).
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == HIDDEN_ARG) {
                tray::show_window(app);
            }
        }));
    // macOS uses SMAppService directly (see autostart.rs).
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .arg(HIDDEN_ARG)
            .build(),
    );
    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Runner::default())
        .setup(|app| setup(app.handle()))
        // Closing the window keeps the app (and reminders) running in the tray.
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                tray::hide_window(window.app_handle());
            }
            // Also save on blur so geometry survives a logout or crash
            // while the window is open.
            WindowEvent::Focused(false) => tray::save_window_state(window.app_handle()),
            #[cfg(not(target_os = "macos"))]
            WindowEvent::ThemeChanged(_) => tray::update_icon(window.app_handle()),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            start_run,
            write_input,
            set_waiting_input,
            resize_pty,
            stop_run,
            list_runs,
            replay_run,
            app_version,
            updater::check_for_update,
            updater::install_update,
            get_settings,
            set_settings,
            get_sources,
            detect_sources,
            get_checks,
            run_checks,
            replay_fixture
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } => tray::save_window_state(app),
            tauri::RunEvent::Exit => app.state::<Runner>().kill(),
            // macOS: clicking the Dock icon while the window is hidden.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_window(app),
            _ => {}
        });
}
