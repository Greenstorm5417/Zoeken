/** Manual theme control persisted in localStorage, applied via `data-theme`. */

export type Theme = "system" | "light" | "dark";

const STORAGE_KEY = "zoeken-theme";

export function getStoredTheme(): Theme {
	if (typeof localStorage === "undefined") return "system";
	const value = localStorage.getItem(STORAGE_KEY);
	return value === "light" || value === "dark" ? value : "system";
}

/** Stamp (or clear) `data-theme` on the document root so CSS overrides win. */
export function applyTheme(theme: Theme) {
	if (typeof document === "undefined") return;
	const root = document.documentElement;
	if (theme === "system") {
		root.removeAttribute("data-theme");
	} else {
		root.setAttribute("data-theme", theme);
	}
}

export function setTheme(theme: Theme) {
	if (typeof localStorage !== "undefined") {
		localStorage.setItem(STORAGE_KEY, theme);
	}
	applyTheme(theme);
}

/** Apply the stored theme immediately (called once at startup). */
export function initTheme() {
	applyTheme(getStoredTheme());
}
