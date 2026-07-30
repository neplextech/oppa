export type AgentState =
  | 'unconfigured'
  | 'authorizing'
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'degraded'
  | 'shutting_down';

export type GatewayState = 'offline' | 'connecting' | 'online';

export type JobState = 'queued' | 'received' | 'submitted' | 'failed' | 'cancelled';

export interface ProductSummary {
  id: string;
  name: string;
  description: string;
  documentationUrl: string;
  supportUrl: string;
}

export interface OpenPrinterServerConfiguration {
  authorizationUrl: string;
  tokenUrl: string;
  gatewayUrl: string;
}

export interface ConnectedServiceSummary {
  name: string;
  serverId: string;
  serverVersion: string;
  gatewayUrl: string;
}

export interface AgentStatus {
  agentId: string | null;
  configured: boolean;
  product: ProductSummary;
  state: AgentState;
  gatewayState: GatewayState;
  lastConnectionAt: string | null;
  version: string;
  pendingJobs: number;
  activeErrors: string[];
  startOnLogin: boolean;
  dashboardUrl: string | null;
  platform: string;
  serverConfiguration: OpenPrinterServerConfiguration;
  connectedService: ConnectedServiceSummary | null;
}

export type PrinterConnectionType = 'system_queue' | 'network' | 'virtual' | 'usb';

export interface PrinterCapabilities {
  widths: Array<58 | 80>;
  documentTypes: Array<'esc_pos' | 'raster' | 'native' | 'virtual'>;
  supportsCut: boolean;
  supportsQr: boolean;
}

export interface PrinterSummary {
  id: string;
  displayName: string;
  sourceName: string;
  connectionType: PrinterConnectionType;
  address: string | null;
  enabled: boolean;
  available: boolean;
  isVirtual: boolean;
  capabilities: PrinterCapabilities | null;
}

export type VirtualPrinterMode = 'always_succeed' | 'fail_next' | 'always_fail' | 'delay' | 'offline';

export interface VirtualOutput {
  id: string;
  jobId: string;
  createdAt: string;
  format: 'structured' | 'esc_pos' | 'raster';
  preview: string;
  byteLength: number;
}

export interface VirtualPrinterSummary extends PrinterSummary {
  isVirtual: true;
  mode: VirtualPrinterMode;
  delayMs: number;
  history: VirtualOutput[];
}

export interface JobSummary {
  id: string;
  printerId: string;
  printerName: string;
  idempotencyKey: string;
  state: JobState;
  receivedAt: string;
  updatedAt: string;
  attempts: number;
  error: string | null;
}

export interface Diagnostics {
  agentVersion: string;
  productId: string;
  platform: string;
  connectionState: AgentState;
  databaseHealthy: boolean;
  migrationVersion: number;
  discoveryProviders: Array<{
    name: string;
    available: boolean;
    lastScanAt: string | null;
    detail: string;
  }>;
  logs: Array<{
    timestamp: string;
    level: 'info' | 'warn' | 'error';
    target: string;
    message: string;
  }>;
}

export interface AuthorizationStart {
  authorizationUrl: string;
  expiresAt: string;
}

export interface ManualPrinterInput {
  displayName: string;
  host: string;
  port: number;
}

export interface VirtualPrinterInput {
  displayName: string;
  width: 58 | 80;
}

export function isVirtualPrinter(printer: PrinterSummary): printer is VirtualPrinterSummary {
  return printer.isVirtual && 'mode' in printer && 'delayMs' in printer && 'history' in printer;
}
