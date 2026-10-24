import { getCurrentWindow } from "@tauri-apps/api/window";

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
  // Native title bar and dialogs follow too; null means "follow the system".
  getCurrentWindow()
    .setTheme(theme === "system" ? null : theme)
    .catch(() => {});
}
