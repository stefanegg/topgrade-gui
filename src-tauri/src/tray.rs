//! Tray icon, window show/hide, and the "update overdue" reminder.

use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

use crate::history::{self, RunSummary};
use crate::settings::Settings;

/// Event asking the frontend to start a run with the saved default flags
/// (the frontend owns the terminal size).
pub const RUN_REQUESTED: &str = "run-requested";

const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Don't repeat the reminder more often than this.
const RENOTIFY_AFTER: Duration = Duration::from_secs(20 * 60 * 60);

/// Shared app state for tray and settings.
pub struct AppState {
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    last_run_item: Mutex<Option<MenuItem<Wry>>>,
    last_notified: Mutex<Option<Instant>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, settings: Settings) -> Self {
        Self {
            data_dir,
            settings: Mutex::new(settings),
            last_run_item: Mutex::new(None),
            last_notified: Mutex::new(None),
        }
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let run = MenuItem::with_id(app, "run", "Run now", true, None::<&str>)?;
    let last_run = MenuItem::with_id(app, "last-run", "Last run: never", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Topgrade GUI", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&run, &last_run, &separator, &open, &quit])?;

    // macOS: monochrome template image the system tints for light/dark menu
    // bars. Elsewhere (e.g. KDE) template images aren't supported, so use the
    // colored app icon.
    let (icon, template) = if cfg!(target_os = "macos") {
        (
            include_bytes!("../icons/tray-template.png").as_slice(),
            true,
        )
    } else {
        (include_bytes!("../icons/32x32.png").as_slice(), false)
    };

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(icon)?)
        .icon_as_template(template)
        .tooltip("Topgrade GUI")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "run" => {
                show_window(app);
                let _ = app.emit(RUN_REQUESTED, ());
            }
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    *app.state::<AppState>().last_run_item.lock().unwrap() = Some(last_run);
    refresh(app);
    start_reminder(app.clone());
    Ok(())
}

pub fn show_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Hides to the tray; on macOS also drops the Dock icon while hidden.
pub fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// Updates "Last run: …" in the tray menu; call after a run finishes.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let text = match last_real_run(&state) {
        Some((_, age)) => format!("Last run: {}", format_age(age)),
        None => "Last run: never".to_string(),
    };
    if let Some(item) = state.last_run_item.lock().unwrap().as_ref() {
        let _ = item.set_text(&text);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("Topgrade GUI · {text}")));
    }
}

fn last_real_run(state: &AppState) -> Option<(RunSummary, chrono::Duration)> {
    let run = history::list(&state.log_dir())
        .into_iter()
        .find(|r| !r.flags.iter().any(|f| f == "--dry-run" || f == "-n"))?;
    let started = chrono::DateTime::parse_from_rfc3339(&run.started_at).ok()?;
    let age = chrono::Local::now().signed_duration_since(started);
    Some((run, age))
}

fn format_age(age: chrono::Duration) -> String {
    match (age.num_days(), age.num_hours(), age.num_minutes()) {
        (d, _, _) if d >= 1 => format!("{d}d ago"),
        (_, h, _) if h >= 1 => format!("{h}h ago"),
        (_, _, m) if m >= 1 => format!("{m}m ago"),
        _ => "just now".into(),
    }
}

fn start_reminder(app: AppHandle) {
    thread::spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            refresh(&app);
            check_overdue(&app);
            thread::sleep(CHECK_INTERVAL);
        }
    });
}

fn check_overdue(app: &AppHandle) {
    let state = app.state::<AppState>();
    let days = state.settings.lock().unwrap().remind_after_days;
    let body = match last_real_run(&state) {
        Some((_, age)) if age.num_days() < i64::from(days) => return,
        Some((_, age)) => format!(
            "Last update was {}. Use Run now in the tray menu.",
            format_age(age)
        ),
        None => "No update run recorded yet. Use Run now in the tray menu.".into(),
    };
    let mut last = state.last_notified.lock().unwrap();
    if last.is_some_and(|t| t.elapsed() < RENOTIFY_AFTER) {
        return;
    }
    *last = Some(Instant::now());
    let _ = app
        .notification()
        .builder()
        .title("Time to update")
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(format_age(chrono::Duration::seconds(20)), "just now");
        assert_eq!(format_age(chrono::Duration::minutes(5)), "5m ago");
        assert_eq!(format_age(chrono::Duration::hours(3)), "3h ago");
        assert_eq!(format_age(chrono::Duration::hours(50)), "2d ago");
    }
}
