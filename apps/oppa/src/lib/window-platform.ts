/** Returns whether the frontend is running inside a Tauri webview. */
export function isTauriEnvironment() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Returns whether the current desktop platform is macOS. */
export function isMacOSPlatform() {
  if (typeof navigator === 'undefined') return false;
  return navigator.platform.toLowerCase().startsWith('mac') || /Mac OS X|Macintosh/.test(navigator.userAgent);
}

/** Returns whether the current Tauri app is running on Windows. */
export function isWindowsTauri() {
  if (typeof navigator === 'undefined') return false;
  const windows = navigator.platform.toLowerCase().startsWith('win') || /Windows/.test(navigator.userAgent);
  return windows && isTauriEnvironment();
}
