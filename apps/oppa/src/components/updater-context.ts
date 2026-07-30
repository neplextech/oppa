import { createContext, useContext } from 'react';

export interface UpdaterContextValue {
  isChecking: boolean;
  lastResult: string | null;
  checkForUpdates: () => Promise<void>;
}

export const UpdaterContext = createContext<UpdaterContextValue | null>(null);

export function useUpdater(): UpdaterContextValue {
  const context = useContext(UpdaterContext);
  if (!context) {
    throw new Error('useUpdater must be used inside UpdaterProvider.');
  }
  return context;
}
