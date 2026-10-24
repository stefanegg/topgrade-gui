/** macOS keeps its native window buttons (overlay title bar); Linux has no
 *  title bar at all and the app draws its own (see tauri.*.conf.json). */
export const isMac = navigator.userAgent.includes("Mac");
