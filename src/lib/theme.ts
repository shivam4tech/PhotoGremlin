/**
 * Appearance (theme) is a frontend-owned preference: it is stored in the
 * webview's local storage and applied as a `data-theme` attribute on
 * <html>, which the CSS token block in theme.css reacts to. The default is
 * dark — the photography "darkroom" look ships as the default experience.
 */
export type Theme = "dark" | "light";

const STORAGE_KEY = "photogremlin.theme";

export const DEFAULT_THEME: Theme = "dark";

export function readStoredTheme(): Theme {
  try {
    return localStorage.getItem(STORAGE_KEY) === "light" ? "light" : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function persistTheme(theme: Theme): void {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage unavailable (private mode / quota): the theme still applies
    // for this session, it just won't be remembered.
  }
}

export function applyTheme(theme: Theme): void {
  // Node-safe: unit tests run without a document.
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
}
