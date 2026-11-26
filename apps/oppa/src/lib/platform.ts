/**
 * Platform detection and capability helpers.
 *
 * `AgentStatus.capabilities`, reported by the Rust host, is authoritative for
 * what the agent can do. {@link detectMobileOs} exists only for decisions that
 * must be made before the first status arrives (initial layout, desktop-only
 * plugins), and never enables a feature by itself.
 */

export type PlatformKind = 'macos' | 'windows' | 'linux' | 'android' | 'ios' | 'other';
export type BackgroundExecution = 'tray' | 'foreground_service' | 'foreground_only';
export type UpdateChannel = 'signed_desktop' | 'apk_download' | 'none';

/** Host capabilities; mirrors `PlatformCapabilities` in the Tauri host. */
export interface PlatformCapabilities {
  kind: PlatformKind;
  mobile: boolean;
  windowControls: boolean;
  systemTray: boolean;
  startOnLogin: boolean;
  systemPrintQueues: boolean;
  networkServiceDiscovery: boolean;
  backgroundExecution: BackgroundExecution;
  keepAwake: boolean;
  updates: UpdateChannel;
  diagnosticsFileExport: boolean;
  quit: boolean;
  usbPrinting: boolean;
}

/** Desktop capabilities used by the browser preview and as a pre-status default. */
export const DESKTOP_CAPABILITIES: PlatformCapabilities = {
  kind: 'other',
  mobile: false,
  windowControls: true,
  systemTray: true,
  startOnLogin: true,
  systemPrintQueues: true,
  networkServiceDiscovery: false,
  backgroundExecution: 'tray',
  keepAwake: false,
  updates: 'signed_desktop',
  diagnosticsFileExport: true,
  quit: true,
  usbPrinting: false,
};

/** Returns the mobile OS hosting this webview, from the user agent. */
export function detectMobileOs(): 'android' | 'ios' | null {
  if (typeof navigator === 'undefined') return null;
  const agent = navigator.userAgent;
  if (/Android/i.test(agent)) return 'android';
  // iPadOS reports a desktop user agent; touch points distinguish it from macOS.
  if (/iPhone|iPad|iPod/i.test(agent) || (/Macintosh/.test(agent) && navigator.maxTouchPoints > 1)) {
    return 'ios';
  }
  return null;
}

/** Whether this webview runs inside the mobile app. */
export function isMobileRuntime(): boolean {
  return detectMobileOs() !== null;
}

/** "this phone", "this computer", … for user-facing copy. */
export function deviceNoun(capabilities: Pick<PlatformCapabilities, 'mobile'>): string {
  return capabilities.mobile ? 'this device' : 'this computer';
}

/** Capitalized device noun for labels such as "Device name". */
export function deviceLabel(capabilities: Pick<PlatformCapabilities, 'mobile'>): string {
  return capabilities.mobile ? 'Device' : 'Computer';
}
