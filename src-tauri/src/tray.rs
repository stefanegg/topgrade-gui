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
const FRAME: Duration = Duration::from_millis(70);
/// Frames per sweep; the glow climbs from the bottom chevron to the top one.
const FRAMES: usize = 14;
/// Alpha the chevrons were drawn with in the static icon (top, middle, bottom).
const BASE_OPACITY: [f32; 3] = [0.9, 0.67, 0.43];
const DIM: f32 = 0.3;
/// Half-width of one chevron's glow, as a fraction of the sweep.
const GLOW: f32 = 0.3;

static ANIMATING: AtomicBool = AtomicBool::new(false);
/// Topgrade is waiting for the user (sudo password, yes/no, retry).
static WAITING: AtomicBool = AtomicBool::new(false);

/// Flags that the run waits for input: the tray icon pulses as a whole
/// instead of sweeping, and if the window isn't in front the user is
/// notified (Dock bounce on macOS).
pub fn set_waiting(app: &AppHandle, waiting: bool, password: bool) {
    if WAITING.swap(waiting, Ordering::SeqCst) == waiting {
        return;
    }
    if let Some(tray) = app.tray_by_id("main") {
        let tip = if waiting {
            "Topgrade GUI · waiting for input"
        } else {
            "Topgrade GUI"
        };
        let _ = tray.set_tooltip(Some(tip));
    }
    if !waiting {
        refresh(app);
        return;
    }
    let window = app.get_webview_window("main");
    if window
        .as_ref()
        .is_some_and(|w| w.is_focused().unwrap_or(false))
    {
        return;
    }
    if let Some(window) = window.filter(|w| w.is_visible().unwrap_or(false)) {
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Critical));
    }
    let body = if password {
        "Topgrade needs your password."
    } else {
        "Topgrade is waiting for your answer."
    };
    let _ = app
        .notification()
        .builder()
        .title("Input needed")
        .body(body)
        .show();
}

/// Brightness of each chevron (top, middle, bottom) at `phase` in 0..1: a
/// smooth pulse that reaches the bottom one first and the top one last.
fn levels(phase: f32) -> [f32; 3] {
    let mut out = [DIM; 3];
    for (k, level) in out.iter_mut().enumerate() {
        let centre = (2 - k) as f32 / 4.0;
        let d = (phase - centre).abs();
        let d = d.min(1.0 - d);
        if d < GLOW {
            let pulse = 0.5 * (1.0 + (std::f32::consts::PI * d / GLOW).cos());
            *level = DIM + (1.0 - DIM) * pulse;
        }
    }
    out
}

/// Redraws the static icon with each chevron at the given strength.
fn frame(base: &Image<'_>, levels: [f32; 3]) -> Image<'static> {
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
            let a = &mut rgba[(y * w + x) * 4 + 3];
            *a = (f32::from(*a) / BASE_OPACITY[k] * levels[k]).min(255.0) as u8;
        }
    }
    Image::new_owned(rgba, w as u32, h as u32)
}

fn set_icon(app: &AppHandle, icon: Image<'static>, template: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        // On macOS a plain `set_icon` resets the template flag, so setting it
        // afterwards makes every frame flash untinted. Set both at once.
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_with_as_template(Some(icon), template);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = tray.set_icon(Some(icon));
            let _ = template;
        }
    }
}

/// Animates the tray icon while a run is active, then restores it.
pub fn animate_while_running(app: &AppHandle) {
    if ANIMATING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        let (base, template) = tray_icon(&app);
        let frames: Vec<_> = (0..FRAMES)
            .map(|i| frame(&base, levels(i as f32 / FRAMES as f32)))
            .collect();
        let alert: Vec<_> = (0..FRAMES)
            .map(|i| {
                let t = i as f32 / FRAMES as f32;
                let level = DIM + (1.0 - DIM) * 0.5 * (1.0 - (std::f32::consts::TAU * t).cos());
                frame(&base, [level; 3])
            })
            .collect();
        let mut step = 0;
        while app.state::<Runner>().is_running() {
            let set = if WAITING.load(Ordering::SeqCst) {
                &alert
            } else {
                &frames
            };
            set_icon(&app, set[step % FRAMES].clone(), template);
            step += 1;
            thread::sleep(FRAME);
        }
        let (icon, template) = tray_icon(&app);
        set_icon(&app, icon, template);
        WAITING.store(false, Ordering::SeqCst);
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
