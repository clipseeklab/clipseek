/** Theme management: auto follows system changes; light/dark override them. */
export type ThemeMode = "auto" | "light" | "dark";
let mediaQuery: MediaQueryList | null = null;
let listener: ((e: MediaQueryListEvent) => void) | null = null;
function apply(isDark: boolean) {
  document.documentElement.classList.toggle("dark", isDark);
}
export function applyTheme(mode: ThemeMode) {
  if (mediaQuery && listener) {
    mediaQuery.removeEventListener("change", listener);
    mediaQuery = null;
    listener = null;
  }
  if (mode === "auto") {
    mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    listener = (e) => apply(e.matches);
    mediaQuery.addEventListener("change", listener);
    apply(mediaQuery.matches);
  } else {
    apply(mode === "dark");
  }
}
