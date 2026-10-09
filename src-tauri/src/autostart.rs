//! Start at login.
//!
//! macOS: `SMAppService.mainApp` registers the app itself, so it shows as
//! "Topgrade GUI" under "Open at Login". (The autostart plugin's LaunchAgent
//! lands under "Allow in the Background", and its AppleScript mode gets
//! credited to "System Events".) Elsewhere: the autostart plugin (an XDG
//! autostart entry on Linux).

use tauri::AppHandle;

/// Makes the login item match `enabled`. Skipped in dev builds, which would
/// register the debug binary.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    platform::apply(app, enabled)
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};
    use tauri::AppHandle;

    pub fn apply(_app: &AppHandle, enabled: bool) -> Result<(), String> {
        // SAFETY: plain Objective-C calls on the main app's service object.
        unsafe {
            let service = SMAppService::mainAppService();
            let status = service.status();
            let registered = status == SMAppServiceStatus::Enabled
                || status == SMAppServiceStatus::RequiresApproval;
            if registered == enabled {
                return Ok(());
            }
            let result = if enabled {
                service.registerAndReturnError()
            } else {
                service.unregisterAndReturnError()
            };
            result.map_err(|e| e.localizedDescription().to_string())
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use tauri::AppHandle;
    use tauri_plugin_autostart::ManagerExt;

    pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), String> {
        let autolaunch = app.autolaunch();
        if autolaunch.is_enabled().unwrap_or(!enabled) == enabled {
            return Ok(());
        }
        let result = if enabled {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        result.map_err(|e| e.to_string())
    }
}
