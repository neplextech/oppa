import { useCallback, useEffect, useState } from 'react';

import { agentClient } from '@/lib/agent-client';
import type { MobileLifecycleStatus } from '@/lib/types';

/** Loads lifecycle status and refreshes it whenever the app becomes visible again. */
export function useMobileLifecycle(enabled: boolean) {
  const [lifecycle, setLifecycle] = useState<MobileLifecycleStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!enabled) return;
    try {
      setLifecycle(await agentClient.getMobileLifecycle());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Lifecycle status is unavailable.');
    }
  }, [enabled]);
  useEffect(() => {
    void refresh();
    // Returning from a system settings page should reflect the new state.
    const onVisible = () => {
      if (document.visibilityState === 'visible') void refresh();
    };
    document.addEventListener('visibilitychange', onVisible);
    window.addEventListener('focus', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('focus', onVisible);
    };
  }, [refresh]);
  return { lifecycle, error, refresh };
}
