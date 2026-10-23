//! Self-update from GitHub Releases via tauri-plugin-updater: the release's
//! signed `latest.json` is checked in the background, the user is asked in the
//! tray menu and in the window, and the update is then downloaded, verified
//! and installed (AppImage/macOS replace in place, RPM/deb go through pkexec).

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::runner::Runner;
use crate::tray::{self, AppState};

/// Tray item clicked: the frontend checks and shows the result.
pub const UPDATE_REQUESTED: &str = "update-requested";
/// A background check found a new version.
pub const UPDATE_AVAILABLE: &str = "update-available";
pub const UPDATE_PROGRESS: &str = "update-progress";

const FIRST_CHECK: Duration = Duration::from_secs(2 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

const CHECK_TEXT: &str = "Check for updates";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    tray_item: Mutex<Option<MenuItem<Wry>>>,
    /// Version a notification was already shown for.
    notified: Mutex<Option<String>>,
}

impl UpdateState {
    pub fn set_tray_item(&self, item: MenuItem<Wry>) {
        *self.tray_item.lock().unwrap() = Some(item);
    }
}

/// Asks GitHub for a newer release and remembers it for `install_update`.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    });
    let state = app.state::<UpdateState>();
    *state.pending.lock().unwrap() = update;
    if let Some(item) = state.tray_item.lock().unwrap().as_ref() {
        let _ = item.set_text(match &info {
            Some(info) => format!("Install update {}…", info.version),
            None => CHECK_TEXT.to_string(),
        });
    }
    Ok(info)
}

pub fn start_background_checks(app: AppHandle) {
    // Dev builds have no signed releases to compare against.
    if cfg!(debug_assertions) {
        return;
    }
    thread::spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            let enabled = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .auto_check_updates;
            if enabled {
                if let Ok(Some(info)) = tauri::async_runtime::block_on(check(&app)) {
                    announce(&app, info);
                }
            }
            thread::sleep(CHECK_INTERVAL);
        }
    });
}

fn announce(app: &AppHandle, info: UpdateInfo) {
    let state = app.state::<UpdateState>();
    let first_time = {
        let mut notified = state.notified.lock().unwrap();
        let first_time = notified.as_deref() != Some(&info.version);
        *notified = Some(info.version.clone());
        first_time
    };
    if first_time {
        let _ = app
            .notification()
            .builder()
            .title("Update available")
            .body(format!(
                "Topgrade GUI {} is available. Choose \"Install update\" in the tray menu.",
                info.version
            ))
            .show();
    }
    let _ = app.emit(UPDATE_AVAILABLE, info);
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    runner: tauri::State<'_, Runner>,
    state: tauri::State<'_, UpdateState>,
) -> Result<(), String> {
    if runner.is_running() {
        return Err("a topgrade run is in progress; install the update when it finishes".into());
    }
    let update = state
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or("no update available")?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(UPDATE_PROGRESS, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // A run may have started during the download; don't kill it. The update
    // is installed and takes effect on the next launch.
    if runner.is_running() {
        return Ok(());
    }
    tray::save_window_state(&app);
    runner.kill();
    app.restart()
}
