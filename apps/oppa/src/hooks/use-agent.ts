import { useCallback, useEffect, useMemo, useState } from 'react';

import { agentClient } from '@/lib/agent-client';
import { isVirtualPrinter } from '@/lib/types';
import type {
  AgentStatus,
  Diagnostics,
  DiscoveredServiceSummary,
  JobSummary,
  ManualPrinterInput,
  OpenPrinterServerConfiguration,
  PrinterSummary,
  VirtualPrinterInput,
  VirtualPrinterMode,
} from '@/lib/types';

export function useAgent() {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [printers, setPrinters] = useState<PrinterSummary[]>([]);
  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [discoveredService, setDiscoveredService] = useState<DiscoveredServiceSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      const [nextStatus, nextPrinters, nextJobs, nextDiagnostics] = await Promise.all([
        agentClient.getStatus(),
        agentClient.listPrinters(),
        agentClient.listRecentJobs(),
        agentClient.getDiagnostics(),
      ]);
      setStatus(nextStatus);
      setPrinters(nextPrinters);
      setJobs(nextJobs);
      setDiagnostics(nextDiagnostics);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The agent did not respond.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;

    void agentClient
      .subscribe(() => {
        void load();
      })
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unsubscribe = stop;
        }
      })
      .catch((cause: unknown) => {
        setError(cause instanceof Error ? cause.message : 'Unable to observe agent updates.');
      });

    return () => {
      disposed = true;
      unsubscribe();
    };
  }, [load]);

  const run = useCallback(async <T>(key: string, operation: () => Promise<T>): Promise<T> => {
    setBusy(key);
    setError(null);
    try {
      return await operation();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'The operation failed.';
      setError(message);
      throw cause;
    } finally {
      setBusy(null);
    }
  }, []);

  const actions = useMemo(
    () => ({
      discoverServer: async () => {
        const result = await run('discover-server', () => agentClient.discoverServer());
        setDiscoveredService(result);
        return result;
      },
      pairServer: async (code: string, agentName: string) => {
        const result = await run('pair-server', () => agentClient.pairServer(code, agentName));
        setDiscoveredService(result);
        await load();
        return result;
      },
      forgetServer: async () => {
        await run('forget-server', () => agentClient.forgetServer());
        setDiscoveredService(null);
        await load();
      },
      refreshPrinters: async () => {
        const nextPrinters = await run('refresh-printers', () => agentClient.refreshPrinters());
        setPrinters(nextPrinters);
      },
      configurePrinter: async (printerId: string, changes: { displayName?: string; enabled?: boolean }) => {
        const updated = await run(`printer-${printerId}`, () => agentClient.configurePrinter(printerId, changes));
        setPrinters((current) => current.map((printer) => (printer.id === printerId ? updated : printer)));
      },
      addManualPrinter: async (input: ManualPrinterInput) => {
        const created = await run('add-manual-printer', () => agentClient.addManualPrinter(input));
        setPrinters((current) => [...current, created]);
      },
      removePrinter: async (printerId: string) => {
        await run(`remove-${printerId}`, () => agentClient.removePrinter(printerId));
        setPrinters((current) => current.filter((printer) => printer.id !== printerId));
      },
      createVirtualPrinter: async (input: VirtualPrinterInput) => {
        const created = await run('create-virtual-printer', () => agentClient.createVirtualPrinter(input));
        setPrinters((current) => [...current, created]);
      },
      updateVirtualPrinter: async (printerId: string, mode: VirtualPrinterMode, delayMs: number) => {
        const updated = await run(`virtual-${printerId}`, () =>
          agentClient.updateVirtualPrinter(printerId, mode, delayMs),
        );
        setPrinters((current) => current.map((printer) => (printer.id === printerId ? updated : printer)));
      },
      clearVirtualHistory: async (printerId: string) => {
        await run(`clear-${printerId}`, () => agentClient.clearVirtualHistory(printerId));
        setPrinters((current) =>
          current.map((printer) =>
            printer.id === printerId && isVirtualPrinter(printer) ? { ...printer, history: [] } : printer,
          ),
        );
      },
      setVirtualPrinterSound: async (printerId: string, enabled: boolean) => {
        await agentClient.setVirtualPrinterSound(printerId, enabled);
      },
      sendTestPrint: async (printerId: string) => {
        const created = await run(`test-${printerId}`, () => agentClient.sendTestPrint(printerId));
        setJobs((current) => [created, ...current]);
      },
      reconnect: async () => {
        const nextStatus = await run('reconnect', () => agentClient.reconnect());
        setStatus(nextStatus);
      },
      setStartOnLogin: async (enabled: boolean) => {
        const nextValue = await run('start-on-login', () => agentClient.setStartOnLogin(enabled));
        setStatus((current) => (current ? { ...current, startOnLogin: nextValue } : current));
      },
      setServerConfiguration: async (input: OpenPrinterServerConfiguration) => {
        await run('server-configuration', () => agentClient.setServerConfiguration(input));
        await load();
      },
      resetServerConfiguration: async () => {
        await run('reset-server-configuration', () => agentClient.resetServerConfiguration());
        await load();
      },
      exportDiagnostics: async () => {
        return run('export-diagnostics', () => agentClient.exportDiagnostics());
      },
      openProductLink: async (link: 'documentation' | 'support', browserFallbackUrl: string) => {
        await run(`open-${link}`, () => agentClient.openProductLink(link, browserFallbackUrl));
      },
      reload: load,
    }),
    [load, run],
  );

  return {
    status,
    printers,
    jobs,
    diagnostics,
    discoveredService,
    loading,
    busy,
    error,
    actions,
  };
}
