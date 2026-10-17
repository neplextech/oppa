export type JobState = 'queued' | 'received' | 'submitted' | 'failed' | 'cancelled';

export interface ProductSummary {
  id: string;
  name: string;
  description: string;
  documentationUrl: string;
  supportUrl: string;
}

export interface OpenPrinterServerConfiguration {
  serverUrl: string;
}

export type OpenPrinterConnectionState =
  | 'idle'
  | 'discovering'
  | 'discovery_failed'
  | 'unpaired'
  | 'pairing'
  | 'paired'
  | 'connecting'
  | 'authenticating'
  | 'connected'
  | 'authentication_failed'
  | 'credential_revoked';

export interface DiscoveredServiceSummary {
  name: string;
  serverId: string;
  serverVersion: string;
  pairingUrl: string;
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
  product: ProductSummary;
  lastConnectionAt: string | null;
  version: string;
  pendingJobs: number;
  activeErrors: string[];
  startOnLogin: boolean;
  dashboardUrl: string | null;
  platform: string;
  serverConfiguration: OpenPrinterServerConfiguration;
  connectedService: ConnectedServiceSummary | null;
  connectionState: OpenPrinterConnectionState;
}

export type PrinterConnectionType = 'system_queue' | 'network' | 'virtual' | 'usb';

export type PrinterSubmissionMode = { type: 'driver' } | { type: 'raw'; language: 'esc-pos' };

export type VirtualPrinterProfile =
  | { type: 'esc-pos-receipt'; widthMm: 58 | 80 }
  | { type: 'system-driver-page'; pageWidthMm: number; pageHeightMm: number; dpi: number };

/** How a printer receives PDF pages: the platform's pipeline, or pages OPPA rasterized. */
export type PdfRoute = 'native' | 'raster';

/** PDF support advertised for one printer. */
export interface PdfCapabilities {
  route: PdfRoute;
  /** Standard paper names the printer offers, such as `A4`. Empty means unknown. */
  paperSizes: string[];
  customPaper: boolean;
  duplex: boolean;
}

export interface PrinterCapabilities {
  widths: Array<58 | 80>;
  documentTypes: Array<'esc_pos' | 'raster' | 'driver' | 'native' | 'virtual' | 'pdf'>;
  supportsSystemDriver: boolean;
  supportsEscPos: boolean;
  supportsCut: boolean;
  supportsQr: boolean;
  pdf?: PdfCapabilities;
}

/** Paper choice in the PDF test form. `default` keeps each page's own size. */
export type PdfPaperChoice = 'default' | 'a4' | 'a5' | 'letter' | 'legal' | 'custom';

/** Print settings edited in the PDF test form. */
export interface PdfPrintSettings {
  copies: number;
  paper: PdfPaperChoice;
  customWidthMm: number;
  customHeightMm: number;
  orientation: 'auto' | 'portrait' | 'landscape';
  scaling: 'fit' | 'actual';
  duplex: 'simplex' | 'long-edge' | 'short-edge';
  pageRanges: string;
}

/** Form defaults that match the protocol defaults. */
export const DEFAULT_PDF_PRINT_SETTINGS: PdfPrintSettings = {
  copies: 1,
  paper: 'default',
  customWidthMm: 50,
  customHeightMm: 30,
  orientation: 'auto',
  scaling: 'actual',
  duplex: 'simplex',
  pageRanges: '',
};

/** Largest PDF accepted from the desktop form. Mirrors the agent's document limit. */
export const MAX_TEST_PDF_BYTES = 20 * 1024 * 1024;

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
  submissionMode: PrinterSubmissionMode;
  virtualProfile?: VirtualPrinterProfile;
}

export type VirtualPrinterMode = 'always_succeed' | 'fail_next' | 'always_fail' | 'delay' | 'offline';

export type ReceiptWidth = 58 | 80;

export type TextAlignment = 'left' | 'center' | 'right';

export interface PrintTextSection {
  type: 'text';
  value: string;
  align?: TextAlignment;
  bold?: boolean;
}

export interface PrintRowSection {
  type: 'row';
  left: string;
  right: string;
}

export interface PrintDividerSection {
  type: 'divider';
}

export interface PrintImageSection {
  type: 'image';
  mediaType: 'image/png' | 'image/jpeg';
  data: string;
}

export interface PrintQrSection {
  type: 'qr';
  value: string;
}

export interface PrintBarcodeSection {
  type: 'barcode';
  format: 'code128' | 'code39' | 'ean13' | 'upca';
  value: string;
}

export interface PrintFeedSection {
  type: 'feed';
  lines: number;
}

export interface PrintCutSection {
  type: 'cut';
}

export type PrintSection =
  | PrintTextSection
  | PrintRowSection
  | PrintDividerSection
  | PrintImageSection
  | PrintQrSection
  | PrintBarcodeSection
  | PrintFeedSection
  | PrintCutSection;

export interface PrintDocument {
  width: ReceiptWidth;
  sections: PrintSection[];
}

export interface VirtualOutput {
  id: string;
  jobId: string;
  createdAt: string;
  format: 'structured' | 'esc_pos' | 'raster' | 'pdf';
  preview: string;
  byteLength: number;
  imageDataUrl?: string;
  diagnostics?: {
    interpretedCommands: number;
    textLines: number;
    images: number;
    qrCodes: number;
    barcodes: number;
    feedLines: number;
    cutRequested: boolean;
    unsupportedCommands: string[];
  };
  /** Original structured document, when the virtual printer captured one. */
  document?: PrintDocument;
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
  connectionState: OpenPrinterConnectionState;
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

export interface ManualPrinterInput {
  displayName: string;
  host: string;
  port: number;
}

export interface VirtualPrinterInput {
  displayName: string;
  profile: VirtualPrinterProfile;
}

export interface PrinterConfigurationChanges {
  displayName?: string;
  enabled?: boolean;
  submissionMode?: PrinterSubmissionMode;
}

export function isVirtualPrinter(printer: PrinterSummary): printer is VirtualPrinterSummary {
  return printer.isVirtual && 'mode' in printer && 'delayMs' in printer && 'history' in printer;
}

export interface RecentServer {
  serverUrl: string;
  name: string | null;
  pairedAt: string;
}

export interface DeepLinkPairing {
  serverUrl: string;
  pairKey: string;
}
