/**
 * Copies text to the system clipboard.
 *
 * Android System WebView rejects the async Clipboard API on the app's
 * non-secure `http://tauri.localhost` origin, so this falls back to a
 * temporary selection and `document.execCommand('copy')`.
 */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Fall through to the selection-based copy.
  }
  const area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.opacity = '0';
  area.style.pointerEvents = 'none';
  document.body.append(area);
  area.select();
  area.setSelectionRange(0, text.length);
  const copied = document.execCommand('copy');
  area.remove();
  if (!copied) throw new Error('The clipboard is unavailable.');
}
