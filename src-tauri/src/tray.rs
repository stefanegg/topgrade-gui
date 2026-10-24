//! Tray icon, window show/hide, and the "update overdue" reminder.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

use crate::history::{self, RunSummary};
use crate::runner::Runner;
use crate::settings::Settings;
use crate::updater;
use crate::window_state;

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
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Topgrade GUI", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&run, &last_run, &separator, &update, &open, &quit])?;

    let (icon, template) = tray_icon(app);

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(template)
        .tooltip("Topgrade GUI")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "run" => {
                show_window(app);
                let _ = app.emit(RUN_REQUESTED, ());
            }
            "update" => {
                show_window(app);
                let _ = app.emit(updater::UPDATE_REQUESTED, ());
            }
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.state::<updater::UpdateState>().set_tray_item(update);
    *app.state::<AppState>().last_run_item.lock().unwrap() = Some(last_run);
    refresh(app);
    start_reminder(app.clone());
    Ok(())
}

/// macOS: monochrome template image the system tints for light/dark menu
/// bars. Elsewhere template images aren't supported, so pick a flat white or
/// black glyph to match the theme, like the other tray icons.
fn tray_icon(app: &AppHandle) -> (Image<'static>, bool) {
    let (bytes, template) = if cfg!(target_os = "macos") {
        (
            include_bytes!("../icons/tray-template.png").as_slice(),
            true,
        )
    } else if app
        .get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .is_some_and(|t| t == tauri::Theme::Light)
    {
        (include_bytes!("../icons/tray-dark.png").as_slice(), false)
    } else {
        (include_bytes!("../icons/tray-light.png").as_slice(), false)
    };
    (
        Image::from_bytes(bytes)
            .expect("bundled tray icon")
            .to_owned(),
        template,
    )
}

/// Re-picks the tray glyph after a light/dark theme change.
#[cfg(not(target_os = "macos"))]
pub fn update_icon(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(tray_icon(app).0));
    }
}

/// Time each animation frame is shown.
const FRAME: Duration = Duration::from_millis(220);
/// Which chevron is lit in each frame (0 = top, 2 = bottom; `None` = all
/// dim): the glow climbs from the bottom arrow to the top one.
const SWEEP: [Option<usize>; 4] = [Some(2), Some(1), Some(0), None];
/// Alpha the chevrons were drawn with in the static icon (top, middle, bottom).
const BASE_OPACITY: [f32; 3] = [0.9, 0.67, 0.43];
const DIM: f32 = 0.3;

static ANIMATING: AtomicBool = AtomicBool::new(false);

/// Redraws the static icon with only chevron `lit` at full strength.
fn frame(base: &Image<'_>, lit: Option<usize>) -> Image<'static> {
    let (w, h) = (base.width() as usize, base.height() as usize);
    let mut rgba = base.rgba().to_vec();
    for y in 0..h {
        for x in 0..w {
            // The chevrons are parallel 45° lines about the icon's centre
            // column, so `v` is constant along one and picks it out.
            let v = y as f32 - (x as f32 - w as f32 / 2.0 + 0.5).abs();
            let k = if v < 9.0 {
                0
            } else if v < 18.0 {
                1
            } else {
                2
            };
            let level = if lit == Some(k) { 1.0 } else { DIM };
            let a = &mut rgba[(y * w + x) * 4 + 3];
            *a = (f32::from(*a) / BASE_OPACITY[k] * level).min(255.0) as u8;
        }
    }
    Image::new_owned(rgba, w as u32, h as u32)
}

fn set_icon(app: &AppHandle, icon: Image<'static>, template: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(icon));
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(template);
        #[cfg(not(target_os = "macos"))]
        let _ = template;
    }
}

/// Animates the tray icon while a run is active, then restores it.
pub fn animate_while_running(app: &AppHandle) {
    if ANIMATING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        let mut step = 0;
        while app.state::<Runner>().is_running() {
            let (base, template) = tray_icon(&app);
            set_icon(&app, frame(&base, SWEEP[step % SWEEP.len()]), template);
            step += 1;
            thread::sleep(FRAME);
        }
        let (icon, template) = tray_icon(&app);
        set_icon(&app, icon, template);
        ANIMATING.store(false, Ordering::SeqCst);
    });
}

pub fn show_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        if !window.is_visible().unwrap_or(true) {
            window_state::restore(&app.state::<AppState>().data_dir, &window);
        }
        let _ = window.show();
        // X11 reads the taskbar icon from the window, not only from the
        // .desktop file. The default 512px icon doesn't make it into
        // _NET_WM_ICON, so set a smaller one once the window is mapped.
        if let Ok(icon) = Image::from_bytes(include_bytes!("../icons/128x128.png")) {
            if let Err(e) = window.set_icon(icon) {
                eprintln!("could not set window icon: {e}");
            }
        }
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Hides to the tray; on macOS also drops the Dock icon while hidden.
pub fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        save_window_state(app);
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// Remembers the window's geometry while it's visible.
pub fn save_window_state(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            window_state::save(&app.state::<AppState>().data_dir, &window);
        }
    }
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
