import type { UpdateItem } from "./run";

/** Rough grouping for people who don't know package names. Heuristic: an
 *  unknown package lands in "system", which is the safe default. */
export type Category = "apps" | "system" | "firmware";

export const CATEGORIES: { key: Category; title: string; hint: string }[] = [
  { key: "apps", title: "Apps", hint: "Programs you open yourself" },
  { key: "system", title: "System and tools", hint: "Libraries, drivers and command-line tools" },
  { key: "firmware", title: "Firmware", hint: "Takes effect after a restart" },
];

/** One updated item, ready to show. */
export interface Change {
  /** Readable name ("GIMP"). */
  name: string;
  /** Technical name when it differs from `name` ("org.gimp.GIMP"). */
  id: string | null;
  from: string | null;
  to: string | null;
  /** Where it came from, e.g. "Flatpak (system)" or "Toolbx › System packages". */
  source: string;
  category: Category;
  needsRestart: boolean;
}

/** The fields shared by live steps and saved step summaries. */
export interface StepUpdates {
  name: string;
  group: string | null;
  items: UpdateItem[];
}

const SOURCE_NAMES: Record<string, string> = {
  "System update": "System packages",
  Brew: "Homebrew",
  "Brew - Cask": "Homebrew apps",
  "Flatpak User Packages": "Flatpak (user)",
  "Flatpak System Packages": "Flatpak (system)",
  "macOS system update": "macOS",
  "Visual Studio Code extensions": "VS Code extensions",
  "Node Package Manager": "npm",
};

export function sourceName(step: string): string {
  return SOURCE_NAMES[step] ?? step;
}

/** Steps whose items are apps a person opens. */
const APP_STEPS = new Set([
  "Flatpak",
  "Flatpak User Packages",
  "Flatpak System Packages",
  "Brew - Cask",
  "Claude Code",
  "Antigravity CLI",
]);

/** Well-known desktop apps shipped by system package managers. */
const KNOWN_APPS = new Set([
  "firefox",
  "thunderbird",
  "chromium",
  "google-chrome-stable",
  "code",
  "gimp",
  "inkscape",
  "krita",
  "blender",
  "vlc",
  "steam",
  "kate",
  "dolphin",
  "konsole",
  "okular",
  "gwenview",
  "nautilus",
  "gnome-terminal",
  "evolution",
  "obs-studio",
]);

/** Flatpak runtimes, SDKs and extensions are plumbing, not apps. */
const FLATPAK_RUNTIME =
  /^org\.freedesktop\.|\.(Platform|Sdk|BaseApp)(\.|$)|\.(Locale|Debug|Sources)$|\.GL(32)?\.|^org\.gtk\.Gtk3theme\./;

const FIRMWARE = /firmware|microcode|ucode/i;

/** Packages that only take effect after a reboot. */
const RESTART = /^(kernel(-core|-modules)?|linux(-image.*)?|systemd|glibc|libc6)$/;

function capitalize(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** "org.gimp.GIMP" → "GIMP"; "com.spotify.Client" → "Spotify". */
function flatpakName(id: string): string {
  const parts = id.split(".");
  if (parts.length < 3) return id;
  let name = parts.at(-1)!;
  if (/^(Client|App|Desktop|Application)$/i.test(name)) name = parts.at(-2)!;
  return capitalize(name);
}

function categorize(step: string, item: UpdateItem): Category {
  if (FIRMWARE.test(item.name) || step.includes("Firmware")) return "firmware";
  if (step.startsWith("Flatpak")) return FLATPAK_RUNTIME.test(item.name) ? "system" : "apps";
  if (APP_STEPS.has(step) || KNOWN_APPS.has(item.name)) return "apps";
  return "system";
}

export function collectChanges(steps: StepUpdates[]): Change[] {
  const changes: Change[] = [];
  for (const step of steps) {
    const source = step.group
      ? `${step.group} › ${sourceName(step.name)}`
      : sourceName(step.name);
    for (const item of step.items) {
      const category = categorize(step.name, item);
      const name = step.name.startsWith("Flatpak")
        ? flatpakName(item.name)
        : KNOWN_APPS.has(item.name)
          ? capitalize(item.name)
          : item.name;
      changes.push({
        name,
        id: name === item.name ? null : item.name,
        from: item.from,
        to: item.to,
        source,
        category,
        needsRestart:
          category === "firmware" ||
          RESTART.test(item.name) ||
          step.name === "macOS system update",
      });
    }
  }
  return changes;
}

/** "a", "a and b", "a, b and c". */
export function joinNames(names: string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
}

/** Up to `max` distinct names, apps first: what a person would recognise. */
export function highlightNames(changes: Change[], max = 3): string[] {
  const order: Category[] = ["apps", "system", "firmware"];
  const sorted = [...changes].sort(
    (a, b) => order.indexOf(a.category) - order.indexOf(b.category),
  );
  return [...new Set(sorted.map((c) => c.name))].slice(0, max);
}

/** Plain-text list for the clipboard. */
export function changesAsText(changes: Change[]): string {
  return CATEGORIES.map(({ key, title }) => {
    const items = changes.filter((c) => c.category === key);
    if (items.length === 0) return "";
    const lines = items.map((c) => {
      const version = [c.from, c.to].filter(Boolean).join(" → ");
      return `- ${c.id ?? c.name}${version ? ` ${version}` : ""} (${c.source})`;
    });
    return `${title}\n${lines.join("\n")}`;
  })
    .filter(Boolean)
    .join("\n\n");
}
