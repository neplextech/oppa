import { generateKeyPairSync, sign } from 'node:crypto';
import { readFileSync } from 'node:fs';

import {
  EXTENDED_PROTOCOL_VERSION,
  PROTOCOL_VERSION,
  decodeBase64Url,
  decodeGatewayAuthenticationServerMessage,
  decodeServerMessage,
  encodeGatewayAuthenticationResponse,
  type GatewayAuthenticationChallenge,
  type OpenPrinterPairingRequest,
  type PrintJob,
  type ProtocolVersionName,
  type ServerMessage,
} from '@openprinter/protocol';
import { describe, expect, it } from 'vitest';

import {
  createOpenPrinterServer,
  type OpenPrinterServer,
  type OpenPrinterTransportCloseRequest,
} from '../src/index.js';

/*
 * Compatibility matrix for protocol negotiation:
 *
 *   old agent  (offers ["1"])        + new server                 → base protocol, unchanged wire
 *   new agent  (offers ["2", "1"])   + new server                 → extended protocol
 *   new agent  (offers ["2", "1"])   + base-only server (["1"])   → closed before server.hello;
 *                                                                   the agent retries with ["1"]
 */

const fixtureRoot = new URL('../../../protocol/fixtures/', import.meta.url);

function fixture<T>(path: string): T {
  return JSON.parse(readFileSync(new URL(path, fixtureRoot), 'utf8')) as T;
}

interface Credential {
  readonly privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'];
  readonly publicKey: OpenPrinterPairingRequest['credential']['publicKey'];
}

function credential(): Credential {
  const pair = generateKeyPairSync('ed25519');
  const jwk = pair.publicKey.export({ format: 'jwk' });
  return { privateKey: pair.privateKey, publicKey: { kty: 'OKP', crv: 'Ed25519', x: jwk.x! } };
}

function wireRecorder() {
  const sent: string[] = [];
  const closes: OpenPrinterTransportCloseRequest[] = [];
  return {
    sent,
    closes,
    transport: {
      send(message: string) {
        sent.push(message);
      },
      close(request: OpenPrinterTransportCloseRequest) {
        closes.push(request);
      },
    },
  };
}

async function waitFor(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error('Timed out waiting for test state.');
}

function serverWith(protocolVersions?: readonly ProtocolVersionName[]) {
  return createOpenPrinterServer<Record<string, string>>({
    brand: { name: 'Negotiation test service' },
    serverId: 'negotiation-service',
    serverVersion: '1.0.0',
    heartbeatIntervalMs: 60_000,
    heartbeatTimeoutMs: 120_000,
    ...(protocolVersions === undefined ? {} : { protocolVersions }),
  });
}

async function pair(openprinter: OpenPrinterServer<Record<string, string>>, key: Credential) {
  const pairing = await openprinter.createPairingCode();
  const result = await openprinter.pair({
    protocolVersion: PROTOCOL_VERSION,
    code: pairing.code,
    agent: { name: 'Front Counter', version: '1.0.0', platform: 'test', installationId: 'installation-01' },
    credential: { algorithm: 'Ed25519', publicKey: key.publicKey },
  });
  if ('error' in result) throw new Error(result.error.code);
  return result;
}

/** Runs gateway authentication on a fresh socket and returns the session plus its wire recorder. */
async function authenticatedSession(
  openprinter: OpenPrinterServer<Record<string, string>>,
  key: Credential,
  agent: Awaited<ReturnType<typeof pair>>,
) {
  const wire = wireRecorder();
  const session = openprinter.accept({ transport: wire.transport });
  await waitFor(() => wire.sent.length === 1);
  const challenge = decodeGatewayAuthenticationServerMessage(wire.sent[0]!) as GatewayAuthenticationChallenge;
  const signature = sign(null, decodeBase64Url(challenge.payload), key.privateKey).toString('base64url');
  await session.receive(
    encodeGatewayAuthenticationResponse({
      type: 'auth.response',
      challengeId: challenge.challengeId,
      agentId: agent.agentId,
      keyId: agent.keyId,
      algorithm: 'Ed25519',
      signature,
    }),
  );
  await waitFor(() => wire.sent.length === 2);
  return { session, wire };
}

/** Sends a hello and waits for the server's reply or close. */
async function hello(
  session: { receive(message: string): Promise<void> },
  wire: ReturnType<typeof wireRecorder>,
  agentId: string,
  offered: readonly ProtocolVersionName[],
): Promise<ServerMessage | null> {
  const sentBefore = wire.sent.length;
  const message = fixture<Record<string, unknown>>('agent/agent-hello.json');
  await session.receive(
    JSON.stringify({
      ...message,
      messageId: `hello-${offered.join('-')}`,
      payload: { ...(message.payload as object), agentId, supportedProtocolVersions: offered },
    }),
  );
  await waitFor(() => wire.sent.length > sentBefore || wire.closes.length > 0);
  if (wire.sent.length === sentBefore) return null;
  return decodeServerMessage(wire.sent[sentBefore]!);
}

function receiptPrinter(id: string) {
  return {
    id,
    fingerprint: `system:CUPS:${id}`,
    name: 'Front desk receipt',
    kind: 'local',
    connection: { type: 'system', systemName: id },
    capabilities: {
      mediaWidths: [80],
      systemDriver: false,
      escPos: true,
      raster: false,
      cut: true,
      qr: true,
      barcode: true,
    },
    enabled: true,
    availability: 'online',
  };
}

function officePrinter(id: string) {
  return {
    ...receiptPrinter(id),
    name: 'Office laser',
    capabilities: {
      mediaWidths: [80],
      systemDriver: true,
      escPos: false,
      raster: true,
      cut: false,
      qr: true,
      barcode: true,
      documentFormats: ['receipt', 'pdf'],
      paperSizes: ['a4', 'letter'],
      customPaperSize: true,
      printOptions: { copies: true, duplex: false, orientation: true, scaling: true, pageRanges: true },
    },
  };
}

function inventory(version: ProtocolVersionName, printers: unknown[]): string {
  return JSON.stringify({
    protocolVersion: version,
    messageId: `inventory-${version}-${printers.length}`,
    sentAt: '2026-08-01T09:01:00.000Z',
    type: 'agent.printer_inventory',
    payload: { revision: 1, printers },
  });
}

function receiptJob(): PrintJob {
  return fixture<{ payload: PrintJob }>('server/print-job.json').payload;
}

function pdfJob(): PrintJob {
  return fixture<{ payload: PrintJob }>('server/print-job-pdf.json').payload;
}

describe('old agent with a new server', () => {
  it('negotiates the base protocol with a handshake identical to the original wire shape', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);

    const reply = await hello(session, wire, agent.agentId, [PROTOCOL_VERSION]);

    expect(reply).toMatchObject({
      protocolVersion: PROTOCOL_VERSION,
      type: 'server.hello',
      payload: { supportedProtocolVersions: [PROTOCOL_VERSION], selectedProtocolVersion: PROTOCOL_VERSION },
    });
    expect(session.getAgent()?.protocolVersion).toBe(PROTOCOL_VERSION);
  });

  it('accepts base-protocol inventory and delivers receipts with base envelopes', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);
    await hello(session, wire, agent.agentId, [PROTOCOL_VERSION]);

    await session.receive(inventory(PROTOCOL_VERSION, [receiptPrinter('printer_receipt_001')]));
    expect(session.getPrinters()).toHaveLength(1);

    const delivered = await session.sendJob(receiptJob());
    expect(delivered).toMatchObject({ ok: true });
    const outbound = JSON.parse(wire.sent[wire.sent.length - 1]!) as { payload: { document: object } };
    expect(outbound).toMatchObject({ type: 'server.print_job', protocolVersion: PROTOCOL_VERSION });
    expect(outbound.payload.document).not.toHaveProperty('format');
  });

  it('refuses PDF jobs without sending them, because the session cannot carry them', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);
    await hello(session, wire, agent.agentId, [PROTOCOL_VERSION]);
    const sentBefore = wire.sent.length;

    const result = await session.sendJob(pdfJob());

    expect(result).toEqual({
      ok: false,
      agentId: agent.agentId,
      reason: 'protocol-version-unsupported',
      retryable: false,
    });
    expect(wire.sent).toHaveLength(sentBefore);
  });

  it('closes the session when it sends extended content, even under a labelled extended envelope', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);
    await hello(session, wire, agent.agentId, [PROTOCOL_VERSION]);

    await session.receive(inventory(EXTENDED_PROTOCOL_VERSION, [officePrinter('printer_office_001')]));

    expect(session.state).toBe('closed');
    expect(session.getPrinters()).toHaveLength(0);
  });
});

describe('new agent with a new server', () => {
  it('negotiates the extended protocol and exchanges document capabilities', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);

    const reply = await hello(session, wire, agent.agentId, [EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION]);

    expect(reply).toMatchObject({
      protocolVersion: PROTOCOL_VERSION,
      type: 'server.hello',
      payload: {
        supportedProtocolVersions: [EXTENDED_PROTOCOL_VERSION],
        selectedProtocolVersion: EXTENDED_PROTOCOL_VERSION,
      },
    });
    expect(session.getAgent()?.protocolVersion).toBe(EXTENDED_PROTOCOL_VERSION);

    await session.receive(inventory(EXTENDED_PROTOCOL_VERSION, [officePrinter('printer_office_001')]));
    expect(session.getPrinters()[0]?.capabilities).toMatchObject({ documentFormats: ['receipt', 'pdf'] });
  });

  it('rejects document capabilities labelled with the base envelope', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);
    await hello(session, wire, agent.agentId, [EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION]);

    await session.receive(inventory(PROTOCOL_VERSION, [officePrinter('printer_office_001')]));

    expect(session.state).toBe('closed');
  });

  it('delivers PDF jobs with the extended envelope and receipts with the base envelope', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);
    await hello(session, wire, agent.agentId, [EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION]);

    expect(await session.sendJob(pdfJob())).toMatchObject({ ok: true });
    expect(JSON.parse(wire.sent[wire.sent.length - 1]!)).toMatchObject({ protocolVersion: EXTENDED_PROTOCOL_VERSION });

    expect(await session.sendJob(receiptJob())).toMatchObject({ ok: true });
    expect(JSON.parse(wire.sent[wire.sent.length - 1]!)).toMatchObject({ protocolVersion: PROTOCOL_VERSION });
  });
});

describe('new agent with a base-only server', () => {
  it('closes an extended hello without replying, so the agent can fall back to the base protocol', async () => {
    const openprinter = serverWith([PROTOCOL_VERSION]);
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session, wire } = await authenticatedSession(openprinter, key, agent);

    const reply = await hello(session, wire, agent.agentId, [EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION]);

    expect(reply).toBeNull();
    expect(wire.closes).toHaveLength(1);
    expect(session.state).toBe('closed');
  });

  it('completes the base handshake on a fresh connection after the fallback', async () => {
    const openprinter = serverWith([PROTOCOL_VERSION]);
    const key = credential();
    const agent = await pair(openprinter, key);

    const rejected = await authenticatedSession(openprinter, key, agent);
    expect(
      await hello(rejected.session, rejected.wire, agent.agentId, [EXTENDED_PROTOCOL_VERSION, PROTOCOL_VERSION]),
    ).toBeNull();

    const retry = await authenticatedSession(openprinter, key, agent);
    const reply = await hello(retry.session, retry.wire, agent.agentId, [PROTOCOL_VERSION]);

    expect(reply).toMatchObject({
      type: 'server.hello',
      payload: { supportedProtocolVersions: [PROTOCOL_VERSION], selectedProtocolVersion: PROTOCOL_VERSION },
    });
    expect(retry.session.getAgent()?.protocolVersion).toBe(PROTOCOL_VERSION);
  });
});

describe('server configuration', () => {
  it('requires the base protocol so every agent can still connect', () => {
    expect(() => serverWith([EXTENDED_PROTOCOL_VERSION])).toThrow(/include "1"/);
    expect(() => serverWith(['1', '1'])).toThrow(/once/);
  });
});
