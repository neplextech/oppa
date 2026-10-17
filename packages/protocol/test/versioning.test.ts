import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import {
  EXTENDED_PROTOCOL_VERSION,
  PROTOCOL_VERSION,
  ProtocolValidationError,
  UnsupportedProtocolVersionError,
  decodeAgentMessage,
  decodeServerMessage,
  encodeAgentMessage,
  requiredProtocolVersion,
  selectProtocolVersion,
  withoutExtendedCapabilities,
  type PrinterDescriptor,
} from '../src/index.js';

const fixtureRoot = new URL('../../../protocol/fixtures/', import.meta.url);

function fixture(path: string): Record<string, unknown> {
  return JSON.parse(readFileSync(new URL(path, fixtureRoot), 'utf8')) as Record<string, unknown>;
}

function withEnvelope(message: Record<string, unknown>, protocolVersion: string): string {
  return JSON.stringify({ ...message, protocolVersion });
}

const receiptPrinter: PrinterDescriptor = {
  id: 'printer_receipt_001',
  fingerprint: 'system:CUPS:receipt-001',
  name: 'Front desk receipt',
  kind: 'local',
  connection: { type: 'system', systemName: 'receipt-001' },
  capabilities: { mediaWidths: [80], raster: false, cut: true, qr: true, barcode: true },
  enabled: true,
  availability: 'online',
};

const officePrinter: PrinterDescriptor = {
  ...receiptPrinter,
  id: 'printer_office_001',
  capabilities: {
    mediaWidths: [80],
    raster: true,
    cut: false,
    qr: true,
    barcode: true,
    documentFormats: ['receipt', 'pdf'],
  },
};

describe('required protocol version', () => {
  it('uses the base version for handshake frames, receipts, and base inventory', () => {
    expect(requiredProtocolVersion(fixture('agent/agent-hello-extended-offer.json') as never)).toBe(PROTOCOL_VERSION);
    expect(requiredProtocolVersion(fixture('server/server-hello-extended.json') as never)).toBe(PROTOCOL_VERSION);
    expect(requiredProtocolVersion(fixture('server/print-job.json') as never)).toBe(PROTOCOL_VERSION);
    expect(
      requiredProtocolVersion({
        type: 'agent.printer_inventory',
        payload: { revision: 1, printers: [receiptPrinter] },
      }),
    ).toBe(PROTOCOL_VERSION);
  });

  it('requires the extended version for PDF jobs and document capabilities', () => {
    expect(requiredProtocolVersion(fixture('server/print-job-pdf.json') as never)).toBe(EXTENDED_PROTOCOL_VERSION);
    expect(
      requiredProtocolVersion({ type: 'agent.printer_inventory', payload: { revision: 1, printers: [officePrinter] } }),
    ).toBe(EXTENDED_PROTOCOL_VERSION);
    expect(
      requiredProtocolVersion({
        type: 'agent.printer_inventory_changed',
        payload: { revision: 2, added: [], updated: [officePrinter], removedPrinterIds: [] },
      }),
    ).toBe(EXTENDED_PROTOCOL_VERSION);
  });
});

describe('version selection', () => {
  it('selects the extended version only when both peers support it', () => {
    expect(selectProtocolVersion(['1', '2'], ['2', '1'])).toBe('2');
    expect(selectProtocolVersion(['1'], ['2', '1'])).toBe('1');
    expect(selectProtocolVersion(['1', '2'], ['1'])).toBe('1');
    expect(selectProtocolVersion(['2'], ['1'])).toBeNull();
  });
});

describe('envelope rules', () => {
  it('rejects extended content labelled with the base version', () => {
    const job = withEnvelope(fixture('server/print-job-pdf.json'), PROTOCOL_VERSION);
    expect(() => decodeServerMessage(job)).toThrow(ProtocolValidationError);
  });

  it('accepts extended content labelled with the extended version', () => {
    const job = withEnvelope(fixture('server/print-job-pdf.json'), EXTENDED_PROTOCOL_VERSION);
    expect(decodeServerMessage(job)).toMatchObject({ protocolVersion: '2', type: 'server.print_job' });
  });

  it('refuses to encode an envelope that does not match its content', () => {
    const message = decodeServerMessage(withEnvelope(fixture('server/print-job-pdf.json'), EXTENDED_PROTOCOL_VERSION));
    expect(() => encodeAgentMessage({ ...message, protocolVersion: PROTOCOL_VERSION } as never)).toThrow();
  });

  it('requires handshake frames to use the base version', () => {
    const hello = withEnvelope(fixture('agent/agent-hello-extended-offer.json'), EXTENDED_PROTOCOL_VERSION);
    expect(() => decodeAgentMessage(hello)).toThrow(ProtocolValidationError);
  });
});

describe('legacy peers', () => {
  const legacy = { supportedVersions: [PROTOCOL_VERSION] as const };

  it('rejects an extended hello offer, as a base-only peer must', () => {
    const offer = JSON.stringify(fixture('agent/agent-hello-extended-offer.json'));
    expect(() => decodeAgentMessage(offer, legacy)).toThrow(ProtocolValidationError);
  });

  it('accepts the original hello, which every peer understands', () => {
    const original = JSON.stringify(fixture('agent/agent-hello.json'));
    expect(decodeAgentMessage(original, legacy)).toMatchObject({ type: 'agent.hello' });
  });

  it('rejects an extended envelope with the version error a base-only peer reports', () => {
    const job = withEnvelope(fixture('server/print-job.json'), EXTENDED_PROTOCOL_VERSION);
    expect(() => decodeServerMessage(job, legacy)).toThrow(UnsupportedProtocolVersionError);
  });

  it('accepts a base-only session agent message unchanged', () => {
    const original = JSON.stringify(fixture('agent/printer-inventory.json'));
    expect(decodeAgentMessage(original, legacy)).toMatchObject({ type: 'agent.printer_inventory' });
  });
});

describe('projection for base-only sessions', () => {
  it('removes every extended capability and leaves the rest of the descriptor intact', () => {
    const projected = withoutExtendedCapabilities(officePrinter);
    expect(projected.capabilities).toEqual({ mediaWidths: [80], raster: true, cut: false, qr: true, barcode: true });
    expect(projected.id).toBe(officePrinter.id);
    expect(withoutExtendedCapabilities(receiptPrinter)).toBe(receiptPrinter);
  });

  it('produces an inventory that a base-only peer accepts', () => {
    const inventory = {
      protocolVersion: PROTOCOL_VERSION,
      messageId: 'msg_projected_inventory',
      sentAt: '2026-08-01T09:00:00.000Z',
      type: 'agent.printer_inventory',
      payload: { revision: 3, printers: [withoutExtendedCapabilities(officePrinter)] },
    };
    expect(decodeAgentMessage(JSON.stringify(inventory), { supportedVersions: [PROTOCOL_VERSION] })).toMatchObject({
      type: 'agent.printer_inventory',
    });
  });
});
