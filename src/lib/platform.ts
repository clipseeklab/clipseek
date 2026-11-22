// WebView2 exposes Windows in the user agent. Keep the macOS layout untouched.
export const isWindows = /Windows/i.test(navigator.userAgent);
