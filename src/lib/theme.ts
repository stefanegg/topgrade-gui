import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMac } from "./platform";

/** settings.rs `Theme`. */
export type Theme = "system" | "light" | "dark";

/** Mirrored in localStorage so app.html can apply it before the first paint. */
const STORAGE_KEY = "theme";

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage unavailable: the theme still applies once settings load.
  }
  // On macOS the window buttons and dialogs follow too; null means "follow
  // the system". Not on Linux: there the tray icon reads the window theme to
  // match the panel, and the app draws its own window buttons anyway.
  if (isMac) {
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
  }
}
