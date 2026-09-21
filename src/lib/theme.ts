import { listen } from "@tauri-apps/api/event";
import { api } from "./api";

/** Omarchy `colors.toml` keys become CSS variables: `dark_background` → `--dark-background`. */
export function applyTheme(colors: Record<string, string>) {
  const root = document.documentElement;
  for (const [key, value] of Object.entries(colors)) {
    if (value.startsWith("#")) root.style.setProperty(`--${key.replaceAll("_", "-")}`, value);
  }
  if (colors.mode) root.style.colorScheme = colors.mode;
}

export async function syncTheme() {
  applyTheme(await api.theme());
  await listen<Record<string, string>>("theme-changed", (e) => applyTheme(e.payload));
}
