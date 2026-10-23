//! Remembers the main window's size and position (`window.json` in the app
//! data dir). Saved geometry is only restored while the monitor layout is
//! the same as when it was saved; after a resolution or monitor change the
//! window keeps its default size and placement.
//!
//! Native Wayland doesn't let clients read or set their window position, so
//! there only the size is restored and placement is left to the compositor.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

const FILE: &str = "window.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    /// Outer position and inner size, in physical pixels.
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
    /// Monitor layout at save time.
    monitors: Vec<MonitorInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitorInfo {
    name: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

impl From<&Monitor> for MonitorInfo {
    fn from(m: &Monitor) -> Self {
        Self {
            name: m.name().cloned(),
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor(),
        }
    }
}

fn monitors<R: Runtime>(window: &WebviewWindow<R>) -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(MonitorInfo::from)
        .collect();
    // Enumeration order isn't guaranteed to be stable.
    list.sort_by(|a, b| (a.x, a.y, &a.name).cmp(&(b.x, b.y, &b.name)));
    list
}

/// Stores the window's current geometry. A maximized window keeps the
/// previously saved normal size/position so un-maximizing after a restore
/// still lands somewhere sensible.
pub fn save<R: Runtime>(dir: &Path, window: &WebviewWindow<R>) {
    let Ok(maximized) = window.is_maximized() else {
        return;
    };
    // Minimized windows report bogus geometry (e.g. -32000 on Windows).
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let monitors = monitors(window);
    if monitors.is_empty() {
        return;
    }
    let state = match (maximized, load(dir)) {
        (true, Some(prev)) => WindowState {
            maximized,
            monitors,
            ..prev
        },
        _ => {
            let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else {
                return;
            };
            WindowState {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
                maximized,
                monitors,
            }
        }
    };
    if load(dir).as_ref() == Some(&state) {
        return;
    }
    let _ = fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_vec_pretty(&state) {
        let _ = fs::write(dir.join(FILE), json);
    }
}

/// Applies the saved geometry if the monitor layout hasn't changed since it
/// was saved. Call before showing the window.
pub fn restore<R: Runtime>(dir: &Path, window: &WebviewWindow<R>) {
    let Some(state) = load(dir) else {
        return;
    };
    if state.monitors != monitors(window) {
        return;
    }
    let _ = window.set_size(PhysicalSize::new(state.width, state.height));
    if can_position() {
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
    }
    if state.maximized {
        let _ = window.maximize();
    }
}

/// False on native Wayland (GTK picks Wayland whenever `WAYLAND_DISPLAY` is
/// set, unless `GDK_BACKEND` says otherwise).
fn can_position() -> bool {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return true;
    }
    std::env::var("GDK_BACKEND").is_ok_and(|b| b.split(',').next() == Some("x11"))
}

fn load(dir: &Path) -> Option<WindowState> {
    serde_json::from_slice(&fs::read(dir.join(FILE)).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_saved_file() {
        let json = r#"{"x":10,"y":20,"width":1100,"height":750,"maximized":false,
            "monitors":[{"name":"DP-1","x":0,"y":0,"width":2560,"height":1440,"scale":1.0}]}"#;
        let state: WindowState = serde_json::from_str(json).unwrap();
        assert_eq!(
            (state.x, state.y, state.width, state.height),
            (10, 20, 1100, 750)
        );
        assert_eq!(state.monitors[0].name.as_deref(), Some("DP-1"));
    }
}
