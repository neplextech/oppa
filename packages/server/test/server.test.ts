import { readFileSync } from 'node:fs';

import {
  decodeServerMessage,
  encodeAgentMessage,
  MAX_WIRE_MESSAGE_BYTES,
  PROTOCOL_VERSION,
  type PrintJob,
  type PrinterDescriptor,
  type ServerMessage,
} from '@openprinter/protocol';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createOpenPrinterServer,
  deliveryResultOrThrow,
  OpenPrinterDeliveryError,
  OpenPrinterServerConfigurationError,
  type AgentMessageOf,
  type OpenPrinterServerOptions,
  type OpenPrinterSession,
  type OpenPrinterTransport,
  type OpenPrinterTransportCloseRequest,
} from '../src/index.js';

type Metadata = {
  readonly organizationId: string;
};

const activeSessions = new Set<OpenPrinterSession<Metadata>>();

afterEach(async () => {
  await Promise.all([...activeSessions].map((session) => session.disconnect({ reconnect: false })));
  activeSessions.clear();
  vi.useRealTimers();
});

describe('createOpenPrinterServer', () => {
  it('owns no HTTP/WebSocket lifecycle and creates independent sessions', async () => {
    const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as {
      dependencies?: Record<string, string>;
    };
    const connected: string[] = [];
    const server = createOpenPrinterServer<Metadata>({
      ...baseOptions(),
      onAgentConnected: ({ session }) => {
        connected.push(session.sessionId);
      },
    });
    const firstTransport = new RecordingTransport();
    const secondTransport = new RecordingTransport();
    const first = track(
      server.accept({
        sessionId: 'session-first',
        identity: authenticatedIdentity(),
        transport: firstTransport,
      }),
    );
    const second = track(
      server.accept({
        sessionId: 'session-second',
        identity: authenticatedIdentity(),
        transport: secondTransport,
      }),
    );

    expect(manifest.dependencies).not.toHaveProperty('ws');
    expect(server).not.toHaveProperty('handleUpgrade');

    await Promise.all([
      first.receive(encodeAgentMessage(createHello('hello-first'))),
      second.receive(encodeAgentMessage(createHello('hello-second'))),
    ]);

    expect(first.state).toBe('connected');
    expect(second.state).toBe('connected');
    expect(connected).toEqual(['session-first', 'session-second']);
    expect(firstTransport.message('server.hello')).toMatchObject({
      correlationId: 'hello-first',
      payload: {
        brand: { name: 'Acme POS' },
        sessionId: 'session-first',
        selectedProtocolVersion: PROTOCOL_VERSION,
      },
    });
    expect(secondTransport.message('server.hello')?.payload.sessionId).toBe('session-second');
  });

  it('returns session-not-ready before hello instead of inventing a global registry', async () => {
    const { session } = createHarness();
    const result = await session.sendJob(printJob);

    expect(result).toEqual({
      ok: false,
      agentId: 'agent-1',
      reason: 'session-not-ready',
      retryable: true,
    });
    expect(() => deliveryResultOrThrow(result)).toThrowError(OpenPrinterDeliveryError);
  });

  it('serializes concurrent receive calls in invocation order', async () => {
    const firstCallbackStarted = deferred<void>();
    const releaseFirstCallback = deferred<void>();
    const revisions: number[] = [];
    const { session, transport } = createHarness({
      onPrintersChanged: async ({ revision }) => {
        revisions.push(revision);
        if (revision === 1) {
          firstCallbackStarted.resolve();
          await releaseFirstCallback.promise;
        }
      },
    });
    await handshake(session, transport);

    const snapshot = session.receive(
      encodeAgentMessage({
        ...agentEnvelope('inventory-1'),
        type: 'agent.printer_inventory',
        payload: {
          revision: 1,
          printers: [virtualPrinter],
        },
      }),
    );
    const updated: PrinterDescriptor = {
      ...virtualPrinter,
      name: 'Human-friendly virtual printer',
    };
    const change = session.receive(
      encodeAgentMessage({
        ...agentEnvelope('inventory-2'),
        type: 'agent.printer_inventory_changed',
        payload: {
          revision: 2,
          added: [],
          updated: [updated],
          removedPrinterIds: [],
        },
      }),
    );

    await firstCallbackStarted.promise;
    expect(revisions).toEqual([1]);
    expect(session.getAgent()?.printerRevision).toBe(1);
    releaseFirstCallback.resolve();
    await Promise.all([snapshot, change]);

    expect(revisions).toEqual([1, 2]);
    expect(session.getAgent()?.printerRevision).toBe(2);
    expect(session.getPrinters()[0]?.name).toBe('Human-friendly virtual printer');
  });

  it('routes job lifecycle messages and hands application commands to the transport', async () => {
    const received = deferred<AgentMessageOf<'agent.job_received'>>();
    const submitted = deferred<AgentMessageOf<'agent.job_submitted'>>();
    const failed = deferred<AgentMessageOf<'agent.job_failed'>>();
    const { session, transport } = createHarness({
      onJobReceived: ({ message }) => received.resolve(message),
      onJobSubmitted: ({ message }) => submitted.resolve(message),
      onJobFailed: ({ message }) => failed.resolve(message),
    });
    await handshake(session, transport);

    const delivery = await session.sendJob(printJob);
    expect(delivery.ok).toBe(true);
    expect(transport.message('server.print_job')?.payload.jobId).toBe('job-1');
    if (!delivery.ok) {
      throw new Error('Expected transport handoff to succeed.');
    }

    await Promise.all([
      session.receive(
        encodeAgentMessage({
          ...agentEnvelope('received-1'),
          correlationId: delivery.messageId,
          type: 'agent.job_received',
          payload: {
            jobId: 'job-1',
            idempotencyKey: 'invoice-1',
            status: 'received',
            receivedAt: new Date().toISOString(),
          },
        }),
      ),
      session.receive(
        encodeAgentMessage({
          ...agentEnvelope('submitted-1'),
          correlationId: delivery.messageId,
          type: 'agent.job_submitted',
          payload: {
            jobId: 'job-1',
            idempotencyKey: 'invoice-1',
            printerId: 'printer-1',
            status: 'submitted',
            submittedAt: new Date().toISOString(),
          },
        }),
      ),
      session.receive(
        encodeAgentMessage({
          ...agentEnvelope('failed-1'),
          correlationId: delivery.messageId,
          type: 'agent.job_failed',
          payload: {
            jobId: 'job-1',
            idempotencyKey: 'invoice-1',
            status: 'failed',
            failedAt: new Date().toISOString(),
            error: {
              code: 'printer_offline',
              message: 'The printer is offline.',
              retryable: true,
            },
          },
        }),
      ),
    ]);

    expect((await received.promise).payload.status).toBe('received');
    expect((await submitted.promise).payload.status).toBe('submitted');
    expect((await failed.promise).payload.status).toBe('failed');
  });

  it('closes an identity-mismatched handshake without exposing a connected agent', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const disconnected = vi.fn();
    const { session, transport } = createHarness({
      onProtocolError: protocolError.resolve,
      onAgentDisconnected: disconnected,
    });
    const hello = createHello();

    await session.receive(
      encodeAgentMessage({
        ...hello,
        payload: {
          ...hello.payload,
          agentId: 'different-agent',
        },
      }),
    );

    expect((await protocolError.promise).code).toBe('identity-mismatch');
    expect(session.state).toBe('closed');
    expect(session.getAgent()).toBeNull();
    expect(transport.closes[0]?.reason).toBe('protocol-error');
    expect(disconnected).not.toHaveBeenCalled();
  });

  it('reports invalid payloads without exposing their contents', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const { session, transport } = createHarness({
      onProtocolError: protocolError.resolve,
    });
    await handshake(session, transport);

    await session.receive(
      JSON.stringify({
        ...agentEnvelope('bad-1'),
        type: 'agent.job_received',
        payload: {
          privateReceipt: 'must-not-appear-in-errors',
        },
      }),
    );

    const event = await protocolError.promise;
    expect(event.code).toBe('invalid-message');
    expect(event.error.message).not.toContain('must-not-appear-in-errors');
    expect(session.state).toBe('closed');
  });

  it('distinguishes unsupported protocol versions', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const { session } = createHarness({
      onProtocolError: protocolError.resolve,
    });

    await session.receive(
      JSON.stringify({
        ...createHello(),
        protocolVersion: 99,
      }),
    );

    expect((await protocolError.promise).code).toBe('unsupported-protocol-version');
  });

  it('closes a transport that never begins the protocol handshake', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout'],
    });
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const { session, transport } = createHarness({
      handshakeTimeoutMs: 10,
      onProtocolError: protocolError.resolve,
    });

    await vi.advanceTimersByTimeAsync(11);

    expect((await protocolError.promise).code).toBe('handshake-timeout');
    expect(transport.closes.at(-1)?.reason).toBe('protocol-error');
    expect(session.state).toBe('closed');
  });

  it('enforces configured inbound and outbound message limits', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const inbound = createHarness({
      maxMessageBytes: 1_024,
      onProtocolError: protocolError.resolve,
    });
    await handshake(inbound.session, inbound.transport);
    await inbound.session.receive('x'.repeat(1_025));
    expect((await protocolError.promise).code).toBe('message-too-large');

    const outbound = createHarness({
      maxMessageBytes: 1_024,
    });
    await handshake(outbound.session, outbound.transport);
    const largeJob: PrintJob = {
      ...printJob,
      document: {
        width: 80,
        sections: [{ type: 'text', value: 'x'.repeat(2_000) }],
      },
    };
    await expect(outbound.session.sendJob(largeJob)).rejects.toMatchObject({
      code: 'message_too_large',
    });
  });

  it('returns a stable transport-error result and finalizes the session when send rejects', async () => {
    const disconnected =
      deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAgentDisconnected']>>[0]>();
    const { session, transport } = createHarness({
      onAgentDisconnected: disconnected.resolve,
    });
    await handshake(session, transport);
    transport.failType = 'server.print_job';

    const result = await session.sendJob(printJob);

    expect(result).toEqual({
      ok: false,
      agentId: 'agent-1',
      reason: 'transport-error',
      retryable: true,
    });
    expect((await disconnected.promise).reason).toBe('transport-error');
    expect(transport.closes.at(-1)?.reason).toBe('transport-error');
    expect(session.state).toBe('closed');
  });

  it('bounds a stalled transport handoff and returns the same stable failure', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout'],
    });
    const { session, transport } = createHarness({
      heartbeatIntervalMs: 5_000,
      heartbeatTimeoutMs: 6_000,
      transportTimeoutMs: 20,
    });
    await handshake(session, transport);
    transport.stallType = 'server.print_job';

    const delivery = session.sendJob(printJob);
    await vi.advanceTimersByTimeAsync(21);

    await expect(delivery).resolves.toEqual({
      ok: false,
      agentId: 'agent-1',
      reason: 'transport-error',
      retryable: true,
    });
    expect(session.state).toBe('closed');
  });

  it('lets the host report transport closure without asking it to close twice', async () => {
    const disconnected =
      deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAgentDisconnected']>>[0]>();
    const { session, transport } = createHarness({
      onAgentDisconnected: disconnected.resolve,
    });
    await handshake(session, transport);

    await session.transportClosed({
      reason: 'peer-closed',
      detail: 'normal host shutdown',
    });

    expect(await disconnected.promise).toMatchObject({
      reason: 'peer-closed',
      detail: 'normal host shutdown',
    });
    expect(transport.close).not.toHaveBeenCalled();
    expect(session.state).toBe('closed');
  });

  it('expires a session that does not answer protocol heartbeats', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout'],
    });
    const timedOut = deferred<void>();
    const disconnected = deferred<void>();
    const { session, transport } = createHarness({
      heartbeatIntervalMs: 5_000,
      heartbeatTimeoutMs: 6_000,
      transportTimeoutMs: 1_000,
      onHeartbeatTimeout: () => timedOut.resolve(),
      onAgentDisconnected: ({ reason }) => {
        if (reason === 'heartbeat-timeout') {
          disconnected.resolve();
        }
      },
    });
    await handshake(session, transport);

    await vi.advanceTimersByTimeAsync(6_001);
    await Promise.all([timedOut.promise, disconnected.promise]);

    expect(transport.closes.at(-1)?.reason).toBe('heartbeat-timeout');
    expect(session.state).toBe('closed');
  });

  it('isolates stalled callbacks while preserving later receive order', async () => {
    const callbackError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onCallbackError']>>[0]>();
    const inventory = deferred<void>();
    const { session, transport } = createHarness({
      callbackTimeoutMs: 20,
      onJobReceived: () => new Promise<void>(() => undefined),
      onPrintersChanged: () => inventory.resolve(),
      onCallbackError: callbackError.resolve,
    });
    await handshake(session, transport);

    const received = session.receive(
      encodeAgentMessage({
        ...agentEnvelope('received-stalled'),
        correlationId: 'delivery-1',
        type: 'agent.job_received',
        payload: {
          jobId: 'job-1',
          idempotencyKey: 'invoice-1',
          status: 'received',
          receivedAt: new Date().toISOString(),
        },
      }),
    );
    const printers = session.receive(
      encodeAgentMessage({
        ...agentEnvelope('inventory-after-stall'),
        type: 'agent.printer_inventory',
        payload: {
          revision: 1,
          printers: [virtualPrinter],
        },
      }),
    );

    const event = await callbackError.promise;
    await Promise.all([received, printers, inventory.promise]);
    expect(event.callback).toBe('onJobReceived');
    expect(event.error).toMatchObject({ name: 'HostCallbackTimeoutError' });
    expect(session.getPrinters()).toHaveLength(1);
  });

  it('sends a semantic disconnect before requesting host transport closure', async () => {
    const disconnected =
      deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAgentDisconnected']>>[0]>();
    const { session, transport } = createHarness({
      onAgentDisconnected: disconnected.resolve,
    });
    await handshake(session, transport);

    expect(
      await session.disconnect({
        code: 'maintenance',
        reason: 'Maintenance window',
        reconnect: true,
        retryAfterMs: 60_000,
      }),
    ).toBe(true);

    expect(transport.message('server.disconnect')?.payload).toEqual({
      code: 'maintenance',
      reason: 'Maintenance window',
      reconnect: true,
      retryAfterMs: 60_000,
    });
    expect(transport.closes.at(-1)).toEqual({
      reason: 'server-disconnect',
      detail: 'Maintenance window',
    });
    expect((await disconnected.promise).reason).toBe('server-disconnect');
    expect(await session.disconnect()).toBe(false);
  });

  it('rejects invalid server and accepted-session configuration synchronously', () => {
    const unsafeBrandNames = [
      ' Acme POS',
      'Acme POS ',
      'Acme POS\u00a0',
      'Acme\u0007POS',
      'Acme\u0085POS',
      'Acme\u061cPOS',
      'Acme\u200ePOS',
      'Acme\u200fPOS',
      'Acme\ud800POS',
      'Acme\udfffPOS',
      ...Array.from({ length: 5 }, (_, offset) => `Acme${String.fromCodePoint(0x202a + offset)}POS`),
      ...Array.from({ length: 4 }, (_, offset) => `Acme${String.fromCodePoint(0x2066 + offset)}POS`),
    ];
    for (const name of unsafeBrandNames) {
      expect(() =>
        createOpenPrinterServer({
          ...baseOptions(),
          brand: { name },
        }),
      ).toThrowError(OpenPrinterServerConfigurationError);
    }
    expect(() =>
      createOpenPrinterServer({
        ...baseOptions(),
        brand: { name: 'Acme 🖨️' },
      }),
    ).not.toThrow();

    const server = createOpenPrinterServer<Metadata>(baseOptions());
    expect(() =>
      server.accept({
        identity: {
          agentId: 'invalid agent id',
          metadata: { organizationId: 'organization-1' },
        },
        transport: new RecordingTransport(),
      }),
    ).toThrowError(OpenPrinterServerConfigurationError);
  });

  it('retains the protocol-wide maximum as an upper configuration bound', () => {
    expect(() =>
      createOpenPrinterServer({
        ...baseOptions(),
        maxMessageBytes: MAX_WIRE_MESSAGE_BYTES + 1,
      }),
    ).toThrowError(OpenPrinterServerConfigurationError);
  });
});

class RecordingTransport implements OpenPrinterTransport {
  public readonly frames: string[] = [];
  public readonly closes: OpenPrinterTransportCloseRequest[] = [];
  public failType: ServerMessage['type'] | null = null;
  public stallType: ServerMessage['type'] | null = null;

  public readonly send = vi.fn((message: string): Promise<void> => {
    const decoded = decodeServerMessage(message);
    if (decoded.type === this.stallType) {
      return new Promise<void>(() => undefined);
    }
    if (decoded.type === this.failType) {
      return Promise.reject(new Error('host transport unavailable'));
    }
    this.frames.push(message);
    return Promise.resolve();
  });

  public readonly close = vi.fn((request: OpenPrinterTransportCloseRequest): Promise<void> => {
    this.closes.push(request);
    return Promise.resolve();
  });

  public messages(): ServerMessage[] {
    return this.frames.map((frame) => decodeServerMessage(frame));
  }

  public message<Type extends ServerMessage['type']>(type: Type): Extract<ServerMessage, { type: Type }> | undefined {
    return this.messages().find((message): message is Extract<ServerMessage, { type: Type }> => message.type === type);
  }
}

function createHarness(overrides: Partial<OpenPrinterServerOptions<Metadata>> = {}): {
  readonly session: OpenPrinterSession<Metadata>;
  readonly transport: RecordingTransport;
} {
  const transport = new RecordingTransport();
  const server = createOpenPrinterServer<Metadata>({
    ...baseOptions(),
    ...overrides,
  });
  const session = track(
    server.accept({
      sessionId: 'session-1',
      identity: authenticatedIdentity(),
      transport,
    }),
  );
  return { session, transport };
}

function baseOptions(): OpenPrinterServerOptions<Metadata> {
  return {
    brand: { name: 'Acme POS' },
    serverId: 'acme-openprinter',
    serverVersion: '1.2.3',
  };
}

function authenticatedIdentity(): {
  readonly agentId: string;
  readonly metadata: Metadata;
} {
  return {
    agentId: 'agent-1',
    metadata: { organizationId: 'organization-1' },
  };
}

function track(session: OpenPrinterSession<Metadata>): OpenPrinterSession<Metadata> {
  activeSessions.add(session);
  return session;
}

async function handshake(session: OpenPrinterSession<Metadata>, transport: RecordingTransport): Promise<void> {
  await session.receive(encodeAgentMessage(createHello()));
  expect(session.state).toBe('connected');
  expect(transport.message('server.hello')).toBeDefined();
  expect(transport.message('server.heartbeat')).toBeDefined();
}

function createHello(messageId = 'hello-1'): AgentMessageOf<'agent.hello'> {
  return {
    ...agentEnvelope(messageId),
    type: 'agent.hello',
    payload: {
      agentId: 'agent-1',
      agentVersion: '0.1.0',
      productId: 'oppa',
      productVersion: '0.1.0',
      supportedProtocolVersions: [PROTOCOL_VERSION],
    },
  };
}

function agentEnvelope(messageId: string): {
  readonly protocolVersion: typeof PROTOCOL_VERSION;
  readonly messageId: string;
  readonly sentAt: string;
} {
  return {
    protocolVersion: PROTOCOL_VERSION,
    messageId,
    sentAt: new Date().toISOString(),
  };
}

function deferred<Value>(): {
  readonly promise: Promise<Value>;
  readonly resolve: (value: Value) => void;
} {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((innerResolve) => {
    resolve = innerResolve;
  });
  return { promise, resolve };
}

const virtualPrinter: PrinterDescriptor = {
  id: 'printer-1',
  fingerprint: 'virtual:printer-1',
  name: 'Development virtual printer',
  kind: 'virtual',
  connection: { type: 'virtual' },
  capabilities: {
    mediaWidths: [80],
    raster: true,
    cut: true,
    qr: true,
    barcode: true,
  },
  enabled: true,
  availability: 'online',
};

const printJob: PrintJob = {
  jobId: 'job-1',
  idempotencyKey: 'invoice-1',
  printerId: 'printer-1',
  createdAt: '2026-07-28T10:00:00.000Z',
  document: {
    width: 80,
    sections: [{ type: 'text', value: 'Test receipt' }, { type: 'cut' }],
  },
};
