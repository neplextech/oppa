import { generateKeyPairSync, sign } from 'node:crypto';

import {
  PROTOCOL_VERSION,
  decodeBase64Url,
  decodeGatewayAuthenticationServerMessage,
  encodeGatewayAuthenticationResponse,
  type GatewayAuthenticationChallenge,
  type OpenPrinterPairingRequest,
  type PrinterDescriptor,
} from '@openprinter/protocol';
import { describe, expect, it, vi } from 'vitest';

import {
  createOpenPrinterServer,
  type OpenPrinterServer,
  type OpenPrinterServerOptions,
  type OpenPrinterTransportCloseRequest,
  type ServerProtocolErrorEvent,
} from '../src/index.js';

/*
 * Inventory revisions are scoped to one authenticated session. An agent
 * that restarts, reinstalls, or reconnects begins a new session and may
 * restart its counter at any value, including one lower than or equal to
 * the last revision seen on an earlier session. Within one session the
 * revision must never regress, and changed content must advance it.
 */

type Metadata = Record<string, string>;

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

function serverWith(options: Partial<OpenPrinterServerOptions<Metadata>> = {}) {
  return createOpenPrinterServer<Metadata>({
    brand: { name: 'Inventory revision test service' },
    serverId: 'inventory-service',
    serverVersion: '1.0.0',
    heartbeatIntervalMs: 60_000,
    heartbeatTimeoutMs: 120_000,
    ...options,
  });
}

async function pair(openprinter: OpenPrinterServer<Metadata>, key: Credential) {
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

type PairedAgent = Awaited<ReturnType<typeof pair>>;

/** Authenticates a fresh socket and completes the base-protocol hello. */
async function connect(openprinter: OpenPrinterServer<Metadata>, key: Credential, agent: PairedAgent) {
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
  await session.receive(
    JSON.stringify({
      protocolVersion: PROTOCOL_VERSION,
      messageId: 'hello-01',
      sentAt: '2026-08-01T09:00:00.000Z',
      type: 'agent.hello',
      payload: {
        agentId: agent.agentId,
        agentVersion: '1.0.0',
        productId: 'oppa',
        productVersion: '1.0.0',
        supportedProtocolVersions: [PROTOCOL_VERSION],
      },
    }),
  );
  expect(session.state).toBe('connected');
  return { session, wire };
}

function printer(id: string, name = 'Front desk receipt'): PrinterDescriptor {
  return {
    id,
    fingerprint: `system:CUPS:${id}`,
    name,
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

/** Returns the most recent protocol error reported to an `onProtocolError` mock. */
function lastProtocolError(callback: ReturnType<typeof vi.fn>): { code: string; message: string } | undefined {
  const event = callback.mock.lastCall?.[0] as ServerProtocolErrorEvent<Metadata> | undefined;
  return event === undefined ? undefined : { code: event.code, message: event.error.message };
}

let messageSequence = 0;

function snapshot(revision: number, printers: readonly PrinterDescriptor[]): string {
  messageSequence += 1;
  return JSON.stringify({
    protocolVersion: PROTOCOL_VERSION,
    messageId: `inventory-${messageSequence}`,
    sentAt: '2026-08-01T09:01:00.000Z',
    type: 'agent.printer_inventory',
    payload: { revision, printers },
  });
}

function change(
  revision: number,
  delta: { added?: PrinterDescriptor[]; updated?: PrinterDescriptor[]; removedPrinterIds?: string[] },
): string {
  messageSequence += 1;
  return JSON.stringify({
    protocolVersion: PROTOCOL_VERSION,
    messageId: `inventory-change-${messageSequence}`,
    sentAt: '2026-08-01T09:02:00.000Z',
    type: 'agent.printer_inventory_changed',
    payload: {
      revision,
      added: delta.added ?? [],
      updated: delta.updated ?? [],
      removedPrinterIds: delta.removedPrinterIds ?? [],
    },
  });
}

describe('inventory revisions across sessions', () => {
  it('accepts a lower revision from a newly authenticated session after the agent restarts', async () => {
    const onPrintersChanged = vi.fn();
    const openprinter = serverWith({ onPrintersChanged });
    const key = credential();
    const agent = await pair(openprinter, key);

    const first = await connect(openprinter, key, agent);
    await first.session.receive(snapshot(42, [printer('printer_a')]));
    expect(first.session.getAgent()?.printerRevision).toBe(42);
    await first.session.transportClosed({ reason: 'peer-closed' });
    expect(first.session.state).toBe('closed');

    const second = await connect(openprinter, key, agent);
    expect(second.session.getAgent()?.printerRevision).toBeNull();
    expect(second.session.getPrinters()).toEqual([]);

    await second.session.receive(snapshot(1, [printer('printer_b')]));

    expect(second.session.state).toBe('connected');
    expect(second.wire.closes).toEqual([]);
    expect(second.session.getAgent()?.printerRevision).toBe(1);
    expect(second.session.getPrinters().map((item) => item.id)).toEqual(['printer_b']);
    expect(onPrintersChanged).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: 'snapshot', revision: 1, session: second.session }),
    );
  });

  it('accepts an equal revision with different content from a new session', async () => {
    const onPrintersChanged = vi.fn();
    const openprinter = serverWith({ onPrintersChanged });
    const key = credential();
    const agent = await pair(openprinter, key);

    const first = await connect(openprinter, key, agent);
    await first.session.receive(snapshot(1, [printer('printer_a')]));
    await first.session.transportClosed();

    const second = await connect(openprinter, key, agent);
    await second.session.receive(snapshot(1, [printer('printer_a', 'Renamed receipt'), printer('printer_b')]));

    expect(second.session.state).toBe('connected');
    expect(second.session.getPrinters()).toHaveLength(2);
    expect(onPrintersChanged).toHaveBeenCalledTimes(2);
  });

  it('accepts revision zero as the first snapshot of a session', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);
    const { session } = await connect(openprinter, key, agent);

    await session.receive(snapshot(0, []));

    expect(session.state).toBe('connected');
    expect(session.getAgent()?.printerRevision).toBe(0);
  });

  it('does not carry inventory into a new session, so a delta still requires a fresh snapshot', async () => {
    const onProtocolError = vi.fn();
    const openprinter = serverWith({ onProtocolError });
    const key = credential();
    const agent = await pair(openprinter, key);

    const first = await connect(openprinter, key, agent);
    await first.session.receive(snapshot(5, [printer('printer_a')]));
    await first.session.transportClosed();

    const second = await connect(openprinter, key, agent);
    await second.session.receive(change(6, { removedPrinterIds: ['printer_a'] }));

    expect(second.session.state).toBe('closed');
    expect(lastProtocolError(onProtocolError)).toEqual({
      code: 'unexpected-message',
      message: 'An incremental printer inventory requires an initial snapshot.',
    });
  });

  it('keeps concurrent sessions for the same agent independent', async () => {
    const openprinter = serverWith();
    const key = credential();
    const agent = await pair(openprinter, key);

    const older = await connect(openprinter, key, agent);
    const newer = await connect(openprinter, key, agent);
    await older.session.receive(snapshot(9, [printer('printer_a')]));
    await newer.session.receive(snapshot(2, [printer('printer_b')]));
    await older.session.receive(change(10, { added: [printer('printer_c')] }));

    expect(older.session.state).toBe('connected');
    expect(newer.session.state).toBe('connected');
    expect(older.session.getAgent()?.printerRevision).toBe(10);
    expect(newer.session.getAgent()?.printerRevision).toBe(2);
    expect(newer.session.getPrinters().map((item) => item.id)).toEqual(['printer_b']);
  });
});

describe('inventory revisions within one session', () => {
  async function connectedWithSnapshot(options: Partial<OpenPrinterServerOptions<Metadata>> = {}) {
    const openprinter = serverWith(options);
    const key = credential();
    const agent = await pair(openprinter, key);
    const connection = await connect(openprinter, key, agent);
    await connection.session.receive(snapshot(5, [printer('printer_a')]));
    return connection;
  }

  it('rejects a snapshot whose revision regresses', async () => {
    const onProtocolError = vi.fn();
    const { session, wire } = await connectedWithSnapshot({ onProtocolError });

    await session.receive(snapshot(4, [printer('printer_a')]));

    expect(session.state).toBe('closed');
    expect(wire.closes[0]).toMatchObject({ reason: 'protocol-error' });
    expect(lastProtocolError(onProtocolError)).toEqual({
      code: 'unexpected-message',
      message: 'The printer inventory revision regressed.',
    });
  });

  it('ignores an identical snapshot repeated at the same revision', async () => {
    const onPrintersChanged = vi.fn();
    const { session } = await connectedWithSnapshot({ onPrintersChanged });

    await session.receive(snapshot(5, [printer('printer_a')]));

    expect(session.state).toBe('connected');
    expect(onPrintersChanged).toHaveBeenCalledOnce();
  });

  it('rejects changed snapshot content that does not advance the revision', async () => {
    const onProtocolError = vi.fn();
    const { session } = await connectedWithSnapshot({ onProtocolError });

    await session.receive(snapshot(5, [printer('printer_a', 'Renamed receipt')]));

    expect(session.state).toBe('closed');
    expect(lastProtocolError(onProtocolError)).toEqual({
      code: 'unexpected-message',
      message: 'The printer inventory changed without advancing its revision.',
    });
  });

  it.each([5, 3])('rejects an incremental change at revision %i that does not advance', async (revision) => {
    const onProtocolError = vi.fn();
    const { session } = await connectedWithSnapshot({ onProtocolError });

    await session.receive(change(revision, { added: [printer('printer_b')] }));

    expect(session.state).toBe('closed');
    expect(lastProtocolError(onProtocolError)).toEqual({
      code: 'unexpected-message',
      message: 'The incremental printer inventory revision did not advance.',
    });
  });

  it('applies advancing snapshots and changes, including revision gaps', async () => {
    const { session } = await connectedWithSnapshot();

    await session.receive(change(9, { added: [printer('printer_b')] }));
    await session.receive(snapshot(20, [printer('printer_c')]));

    expect(session.state).toBe('connected');
    expect(session.getAgent()?.printerRevision).toBe(20);
    expect(session.getPrinters().map((item) => item.id)).toEqual(['printer_c']);
  });

  it('rejects a regression that follows an incremental change', async () => {
    const { session } = await connectedWithSnapshot();

    await session.receive(change(6, { added: [printer('printer_b')] }));
    await session.receive(snapshot(5, [printer('printer_a')]));

    expect(session.state).toBe('closed');
  });
});
