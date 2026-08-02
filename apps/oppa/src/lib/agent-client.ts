import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  AgentStatus,
  DeepLinkPairing,
  DiscoveredServiceSummary,
  Diagnostics,
  JobSummary,
  ManualPrinterInput,
  OpenPrinterServerConfiguration,
  PrinterSummary,
  RecentServer,
  VirtualPrinterInput,
  VirtualPrinterMode,
  VirtualPrinterSummary,
} from './types';

const now = new Date();

const demoVirtualPrinter: VirtualPrinterSummary = {
  id: 'printer_virtual_receipts',
  displayName: 'Receipt preview',
  sourceName: 'OPPA virtual printer',
  connectionType: 'virtual',
  address: null,
  enabled: true,
  available: true,
  isVirtual: true,
  capabilities: {
    widths: [58, 80],
    documentTypes: ['virtual', 'esc_pos'],
    supportsCut: true,
    supportsQr: true,
  },
  mode: 'always_succeed',
  delayMs: 0,
  history: [
    {
      id: 'output_demo_1',
      jobId: 'job_demo_1042',
      createdAt: new Date(now.getTime() - 1000 * 60 * 4).toISOString(),
      format: 'structured',
      preview: 'OPENPRINTER TEST\nConnection verified\n\n        NPR 1,250.00\n------------------------',
      byteLength: 384,
    },
  ],
};

const demoPrinters: PrinterSummary[] = [
  demoVirtualPrinter,
  {
    id: 'printer_system_frontdesk',
    displayName: 'Front desk',
    sourceName: 'EPSON TM-T82III',
    connectionType: 'system_queue',
    address: 'EPSON_TM_T82III',
    enabled: true,
    available: true,
    isVirtual: false,
    capabilities: {
      widths: [80],
      documentTypes: ['native', 'raster'],
      supportsCut: true,
      supportsQr: true,
    },
  },
  {
    id: 'printer_network_counter',
    displayName: 'Counter backup',
    sourceName: 'Raw TCP printer',
    connectionType: 'network',
    address: '192.168.1.82:9100',
    enabled: false,
    available: false,
    isVirtual: false,
    capabilities: {
      widths: [80],
      documentTypes: ['esc_pos'],
      supportsCut: true,
      supportsQr: true,
    },
  },
];

const demoJobs: JobSummary[] = [
  {
    id: 'job_demo_1042',
    printerId: demoVirtualPrinter.id,
    printerName: demoVirtualPrinter.displayName,
    idempotencyKey: 'invoice_demo_1042_v1',
    state: 'submitted',
    receivedAt: new Date(now.getTime() - 1000 * 60 * 4).toISOString(),
    updatedAt: new Date(now.getTime() - 1000 * 60 * 4 + 220).toISOString(),
    attempts: 1,
    error: null,
  },
  {
    id: 'job_demo_1041',
    printerId: 'printer_system_frontdesk',
    printerName: 'Front desk',
    idempotencyKey: 'invoice_demo_1041_v1',
    state: 'submitted',
    receivedAt: new Date(now.getTime() - 1000 * 60 * 21).toISOString(),
    updatedAt: new Date(now.getTime() - 1000 * 60 * 21 + 860).toISOString(),
    attempts: 1,
    error: null,
  },
  {
    id: 'job_demo_1039',
    printerId: 'printer_network_counter',
    printerName: 'Counter backup',
    idempotencyKey: 'invoice_demo_1039_v2',
    state: 'failed',
    receivedAt: new Date(now.getTime() - 1000 * 60 * 56).toISOString(),
    updatedAt: new Date(now.getTime() - 1000 * 60 * 55).toISOString(),
    attempts: 3,
    error: 'Printer was unavailable before submission.',
  },
];

const demoStatus: AgentStatus = {
  agentId: 'agent_01JOPPA7Y9JD4BVA0N',
  product: {
    id: 'oppa',
    name: 'OPPA',
    description: 'Open Printer Proxy Agent',
    documentationUrl: 'https://github.com/neplextech/oppa',
    supportUrl: 'https://github.com/neplextech/oppa/issues',
  },
  lastConnectionAt: new Date(now.getTime() - 1000 * 32).toISOString(),
  version: '0.1.0',
  pendingJobs: 0,
  activeErrors: [],
  startOnLogin: true,
  dashboardUrl: null,
  platform: navigator.platform || 'Desktop',
  serverConfiguration: {
    serverUrl: 'http://127.0.0.1:8787/',
  },
  connectionState: 'connected',
  connectedService: {
    name: 'Acme POS',
    serverId: 'openprinter-example',
    serverVersion: '0.1.0',
    gatewayUrl: 'ws://127.0.0.1:8787/.well-known/openprinter/gateway',
  },
};

const demoDiagnostics: Diagnostics = {
  agentVersion: demoStatus.version,
  productId: demoStatus.product.id,
  platform: demoStatus.platform,
  connectionState: demoStatus.connectionState,
  databaseHealthy: true,
  migrationVersion: 1,
  discoveryProviders: [
    {
      name: 'System queues',
      available: true,
      lastScanAt: new Date(now.getTime() - 1000 * 48).toISOString(),
      detail: '1 printer discovered',
    },
    {
      name: 'Manual network',
      available: true,
      lastScanAt: new Date(now.getTime() - 1000 * 48).toISOString(),
      detail: '1 printer configured',
    },
    {
      name: 'Virtual',
      available: true,
      lastScanAt: new Date(now.getTime() - 1000 * 48).toISOString(),
      detail: '1 printer configured',
    },
  ],
  logs: [
    {
      timestamp: new Date(now.getTime() - 1000 * 32).toISOString(),
      level: 'info',
      target: 'transport',
      message: 'Authenticated gateway connection established.',
    },
    {
      timestamp: new Date(now.getTime() - 1000 * 48).toISOString(),
      level: 'info',
      target: 'discovery',
      message: 'Printer inventory synchronized.',
    },
    {
      timestamp: new Date(now.getTime() - 1000 * 60 * 4).toISOString(),
      level: 'info',
      target: 'job',
      message: 'Job job_demo_1042 submitted to virtual printer.',
    },
  ],
};

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}

export const agentClient = {
  async subscribe(onChange: () => void): Promise<() => void> {
    if (!isTauri()) {
      return () => undefined;
    }

    const unlisten = await Promise.all(
      ['oppa://state-changed', 'oppa://printers-changed', 'oppa://jobs-changed'].map(async (event) =>
        listen(event, onChange),
      ),
    );
    return () => {
      for (const stop of unlisten) {
        stop();
      }
    };
  },

  async subscribeNavigation(
    onNavigate: (screen: 'overview' | 'printers' | 'virtual' | 'jobs' | 'diagnostics' | 'settings') => void,
  ): Promise<() => void> {
    if (!isTauri()) {
      return () => undefined;
    }
    return listen<string>('oppa://navigate', ({ payload }) => {
      if (
        payload === 'overview' ||
        payload === 'printers' ||
        payload === 'virtual' ||
        payload === 'jobs' ||
        payload === 'diagnostics' ||
        payload === 'settings'
      ) {
        onNavigate(payload);
      }
    });
  },

  async getStatus(): Promise<AgentStatus> {
    return isTauri() ? command('get_agent_status') : structuredClone(demoStatus);
  },

  async discoverServer(): Promise<DiscoveredServiceSummary> {
    if (isTauri()) {
      return command('discover_server');
    }
    return {
      name: 'Acme POS',
      serverId: 'openprinter-example',
      serverVersion: '1.0.0',
      pairingUrl: 'http://127.0.0.1:8787/openprinter/pair',
      gatewayUrl: 'ws://127.0.0.1:8787/.well-known/openprinter/gateway',
    };
  },

  async pairServer(code: string, agentName: string): Promise<DiscoveredServiceSummary> {
    if (isTauri()) {
      return command('pair_server', { code, agentName });
    }
    demoStatus.agentId = 'agt_demo';
    demoStatus.connectionState = 'paired';
    return this.discoverServer();
  },

  async forgetServer(): Promise<void> {
    if (isTauri()) {
      await command('forget_server');
      return;
    }
    demoStatus.agentId = null;
    demoStatus.connectionState = 'unpaired';
    demoStatus.connectedService = null;
  },

  async listPrinters(): Promise<PrinterSummary[]> {
    return isTauri() ? command('list_printers') : structuredClone(demoPrinters);
  },

  async refreshPrinters(): Promise<PrinterSummary[]> {
    return isTauri() ? command('refresh_printers') : structuredClone(demoPrinters);
  },

  async configurePrinter(
    printerId: string,
    changes: { displayName?: string; enabled?: boolean },
  ): Promise<PrinterSummary> {
    if (isTauri()) {
      return command('configure_printer', { printerId, changes });
    }
    const printer = demoPrinters.find((candidate) => candidate.id === printerId);
    if (!printer) {
      throw new Error('Printer no longer exists.');
    }
    return { ...printer, ...changes };
  },

  async addManualPrinter(input: ManualPrinterInput): Promise<PrinterSummary> {
    if (isTauri()) {
      return command('add_manual_printer', { input });
    }
    return {
      id: `printer_network_${Date.now()}`,
      displayName: input.displayName,
      sourceName: 'Raw TCP printer',
      connectionType: 'network',
      address: `${input.host}:${input.port}`,
      enabled: true,
      available: true,
      isVirtual: false,
      capabilities: {
        widths: [80],
        documentTypes: ['esc_pos'],
        supportsCut: true,
        supportsQr: true,
      },
    };
  },

  async removePrinter(printerId: string): Promise<void> {
    if (isTauri()) {
      await command('remove_printer', { printerId });
    }
  },

  async createVirtualPrinter(input: VirtualPrinterInput): Promise<VirtualPrinterSummary> {
    if (isTauri()) {
      return command('create_virtual_printer', { input });
    }
    return {
      ...structuredClone(demoVirtualPrinter),
      id: `printer_virtual_${Date.now()}`,
      displayName: input.displayName,
      capabilities: {
        widths: [input.width],
        documentTypes: ['virtual', 'esc_pos'],
        supportsCut: true,
        supportsQr: true,
      },
      history: [],
    };
  },

  async updateVirtualPrinter(
    printerId: string,
    mode: VirtualPrinterMode,
    delayMs: number,
  ): Promise<VirtualPrinterSummary> {
    if (isTauri()) {
      return command('update_virtual_printer', { printerId, mode, delayMs });
    }
    return { ...structuredClone(demoVirtualPrinter), id: printerId, mode, delayMs };
  },

  async clearVirtualHistory(printerId: string): Promise<void> {
    if (isTauri()) {
      await command('clear_virtual_history', { printerId });
    }
  },

  async setVirtualPrinterSound(printerId: string, enabled: boolean): Promise<void> {
    if (isTauri()) {
      await command('set_virtual_printer_sound', { printerId, enabled });
    }
  },

  async subscribePrinterSound(onSound: (printerId: string) => void): Promise<() => void> {
    if (!isTauri()) return () => undefined;
    return listen<string>('oppa://printer-sound', ({ payload }) => {
      onSound(payload);
    });
  },

  async sendTestPrint(printerId: string): Promise<JobSummary> {
    if (isTauri()) {
      return command('send_test_print', { printerId });
    }
    const printer = demoPrinters.find((candidate) => candidate.id === printerId);
    const createdAt = new Date().toISOString();
    return {
      id: `job_test_${Date.now()}`,
      printerId,
      printerName: printer?.displayName ?? 'Printer',
      idempotencyKey: `test_${crypto.randomUUID()}`,
      state: 'submitted',
      receivedAt: createdAt,
      updatedAt: createdAt,
      attempts: 1,
      error: null,
    };
  },

  async listRecentJobs(): Promise<JobSummary[]> {
    return isTauri() ? command('list_recent_jobs') : structuredClone(demoJobs);
  },

  async clearJobs(): Promise<void> {
    if (isTauri()) {
      await command('clear_jobs');
    }
    demoJobs.length = 0;
  },

  async getDiagnostics(): Promise<Diagnostics> {
    return isTauri() ? command('get_diagnostics') : structuredClone(demoDiagnostics);
  },

  async clearLogs(): Promise<void> {
    if (isTauri()) {
      await command('clear_logs');
    }
    demoDiagnostics.logs.length = 0;
  },

  async exportDiagnostics(): Promise<string> {
    return isTauri() ? command('export_diagnostics') : 'Diagnostics export is available in the desktop application.';
  },

  async setStartOnLogin(enabled: boolean): Promise<boolean> {
    return isTauri() ? command('set_start_on_login', { enabled }) : enabled;
  },

  async reconnect(): Promise<AgentStatus> {
    return isTauri() ? command('reconnect') : structuredClone(demoStatus);
  },

  async setServerConfiguration(input: OpenPrinterServerConfiguration): Promise<OpenPrinterServerConfiguration> {
    if (isTauri()) {
      return command('set_server_configuration', { input });
    }
    demoStatus.serverConfiguration = structuredClone(input);
    demoStatus.agentId = null;
    demoStatus.connectedService = null;
    demoStatus.connectionState = 'unpaired';
    return structuredClone(input);
  },

  async resetServerConfiguration(): Promise<OpenPrinterServerConfiguration> {
    if (isTauri()) {
      return command('reset_server_configuration');
    }
    const configuration = {
      serverUrl: 'http://127.0.0.1:8787/',
    };
    demoStatus.serverConfiguration = configuration;
    demoStatus.agentId = null;
    demoStatus.connectedService = null;
    demoStatus.connectionState = 'unpaired';
    return structuredClone(configuration);
  },

  async openProductLink(link: 'documentation' | 'support', browserFallbackUrl: string): Promise<void> {
    if (isTauri()) {
      await command('open_product_link', { link });
      return;
    }
    window.open(browserFallbackUrl, '_blank', 'noopener,noreferrer');
  },

  async listRecentServers(): Promise<RecentServer[]> {
    return isTauri() ? command('list_recent_servers') : [];
  },

  async applyRecentServer(serverUrl: string): Promise<void> {
    if (isTauri()) {
      await command('apply_recent_server', { serverUrl });
    }
  },

  async getPendingDeepLink(): Promise<DeepLinkPairing | null> {
    return isTauri() ? command<DeepLinkPairing | null>('get_pending_deep_link') : null;
  },

  async subscribeDeepLink(onDeepLink: (data: DeepLinkPairing) => void): Promise<() => void> {
    if (!isTauri()) return () => undefined;
    return listen<DeepLinkPairing>('oppa://deep-link', ({ payload }) => {
      onDeepLink(payload);
    });
  },
};
