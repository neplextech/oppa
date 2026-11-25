import type { DiscoveredServiceSummary } from './types';

/** The three exclusive setup stages. Pairing secrets are intentionally memory-only. */
export type SetupStep = 'connect' | 'pair' | 'printers';

/** Non-secret progress that may be restored after restarting the desktop host. */
export interface SetupProgress {
  step: SetupStep;
  serverUrl: string;
  computerName: string;
}

/** Validates local preferences before they can influence the onboarding view. */
export function readSetupProgress(raw: string | null): SetupProgress | null {
  try {
    const value: unknown = JSON.parse(raw ?? 'null');
    if (!value || typeof value !== 'object') return null;
    if (!('step' in value) || !['connect', 'pair', 'printers'].includes(String(value.step))) return null;
    if (!('serverUrl' in value) || typeof value.serverUrl !== 'string' || value.serverUrl.length > 2048) return null;
    if (!('computerName' in value) || typeof value.computerName !== 'string' || value.computerName.length > 128)
      return null;
    return { step: value.step as SetupStep, serverUrl: value.serverUrl, computerName: value.computerName };
  } catch {
    return null;
  }
}

/** In-memory setup state; service identities are re-discovered after a restart. */
export interface SetupState extends SetupProgress {
  identity: DiscoveredServiceSummary | null;
  code: string;
}

/** Events explicitly control forward and backward navigation. */
export type SetupEvent =
  | { type: 'address'; value: string }
  | { type: 'name'; value: string }
  | { type: 'code'; value: string }
  | { type: 'discovered'; identity: DiscoveredServiceSummary; serverUrl: string }
  | { type: 'paired' }
  | { type: 'reconnected' }
  | { type: 'back' }
  | { type: 'link'; serverUrl: string; code: string };

/** Advances only after successful agent operations; going back preserves safe entered values. */
export function setupReducer(state: SetupState, event: SetupEvent): SetupState {
  switch (event.type) {
    case 'address':
      return {
        ...state,
        serverUrl: event.value,
        identity: null,
        code: event.value === state.serverUrl ? state.code : '',
      };
    case 'name':
      return { ...state, computerName: event.value };
    case 'code':
      return { ...state, code: normalizePairingCode(event.value) };
    case 'discovered':
      return { ...state, step: 'pair', identity: event.identity, serverUrl: event.serverUrl };
    case 'paired':
      return state.step === 'pair' && state.identity ? { ...state, step: 'printers', code: '' } : state;
    case 'reconnected':
      return { ...state, step: 'printers', code: '' };
    case 'back':
      return { ...state, step: 'connect' };
    case 'link':
      return {
        ...state,
        step: 'pair',
        identity: null,
        serverUrl: event.serverUrl,
        // Deep-link keys are opaque server-issued values; preserve their exact casing.
        code: event.code,
      };
  }
}

/** Removes paste whitespace and normalizes letters without discarding meaningful separators. */
export function normalizePairingCode(value: string): string {
  return value.replace(/\s+/g, '').toUpperCase();
}
