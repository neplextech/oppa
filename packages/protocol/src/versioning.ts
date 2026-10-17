import { EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION } from './constants.js';
import type { PROTOCOL_VERSIONS } from './constants.js';
import type { PrinterCapabilities, PrinterDescriptor } from './schemas/printer.js';

/** A protocol version string understood by this package. */
export type ProtocolVersionName = (typeof PROTOCOL_VERSIONS)[number];

/** Message types exchanged before a protocol version is selected. They always use the base version. */
const HANDSHAKE_TYPES: ReadonlySet<string> = new Set(['agent.hello', 'server.hello']);

/** Printer capability keys introduced by the extended protocol. */
const EXTENDED_CAPABILITY_KEYS = ['documentFormats', 'paperSizes', 'customPaperSize', 'printOptions'] as const;

/** Minimal shape shared by every envelope, used to decide which version a message needs. */
interface VersionedEnvelope {
  readonly type: string;
  readonly payload: unknown;
}

/** Returns whether a printer descriptor advertises any capability introduced by the extended protocol. */
export function hasExtendedCapabilities(printer: PrinterDescriptor): boolean {
  const capabilities = printer.capabilities as Partial<PrinterCapabilities> | undefined;
  if (capabilities === undefined) {
    return false;
  }
  return EXTENDED_CAPABILITY_KEYS.some((key) => capabilities[key] !== undefined);
}

/**
 * Returns the lowest protocol version whose contract covers a message.
 *
 * Handshake frames and every message that uses only base-protocol content return `"1"`. A PDF print
 * job or a printer descriptor with extended capabilities requires `"2"`. Peers use this rule to
 * choose envelope versions, so receipts and reports remain byte-identical to the base protocol.
 */
export function requiredProtocolVersion(message: VersionedEnvelope): ProtocolVersionName {
  if (HANDSHAKE_TYPES.has(message.type)) {
    return PROTOCOL_VERSION;
  }
  if (message.type === 'server.print_job') {
    const document = (message.payload as { document?: { format?: string } }).document;
    return document?.format === 'pdf' ? EXTENDED_PROTOCOL_VERSION : PROTOCOL_VERSION;
  }
  if (message.type === 'agent.printer_inventory') {
    const printers = (message.payload as { printers: PrinterDescriptor[] }).printers;
    return printers.some(hasExtendedCapabilities) ? EXTENDED_PROTOCOL_VERSION : PROTOCOL_VERSION;
  }
  if (message.type === 'agent.printer_inventory_changed') {
    const { added, updated } = message.payload as { added: PrinterDescriptor[]; updated: PrinterDescriptor[] };
    return [...added, ...updated].some(hasExtendedCapabilities) ? EXTENDED_PROTOCOL_VERSION : PROTOCOL_VERSION;
  }
  return PROTOCOL_VERSION;
}

/**
 * Selects the highest protocol version both peers support.
 *
 * Returns `null` when the lists share no version. The selection is always taken from the server's
 * own list, so a peer cannot negotiate a version the server did not enable.
 */
export function selectProtocolVersion(
  serverSupported: readonly ProtocolVersionName[],
  offered: readonly ProtocolVersionName[],
): ProtocolVersionName | null {
  if (serverSupported.includes(EXTENDED_PROTOCOL_VERSION) && offered.includes(EXTENDED_PROTOCOL_VERSION)) {
    return EXTENDED_PROTOCOL_VERSION;
  }
  return serverSupported.includes(PROTOCOL_VERSION) && offered.includes(PROTOCOL_VERSION) ? PROTOCOL_VERSION : null;
}

/**
 * Removes capabilities introduced by the extended protocol.
 *
 * A session that negotiated only the base protocol receives printer descriptors projected this way,
 * so it never sees fields its peer cannot decode.
 */
export function withoutExtendedCapabilities(printer: PrinterDescriptor): PrinterDescriptor {
  if (!hasExtendedCapabilities(printer) || printer.capabilities === undefined) {
    return printer;
  }
  const capabilities: Record<string, unknown> = { ...printer.capabilities };
  for (const key of EXTENDED_CAPABILITY_KEYS) {
    delete capabilities[key];
  }
  return { ...printer, capabilities: capabilities as PrinterCapabilities };
}
