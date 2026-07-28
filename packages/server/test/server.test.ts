import { once } from 'node:events';
import { createServer, type Server as HttpServer } from 'node:http';

import {
  decodeServerMessage,
  encodeAgentMessage,
  PROTOCOL_VERSION,
  type AgentMessage,
  type PrintJob,
  type PrinterDescriptor,
  type ServerMessage,
} from '@openprinter/protocol';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebSocket } from 'ws';

import {
  createOpenPrinterServer,
  deliveryResultOrThrow,
  OpenPrinterDeliveryError,
  type AgentMessageOf,
  type OpenPrinterServer,
  type OpenPrinterServerOptions,
} from '../src/index.js';

type Metadata = {
  readonly organizationId: string;
};

interface Harness {
  readonly httpServer: HttpServer;
  readonly openPrinter: OpenPrinterServer<Metadata>;
  readonly port: number;
  readonly clients: Set<WebSocket>;
  close(): Promise<void>;
}

const activeHarnesses = new Set<Harness>();

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all([...activeHarnesses].map((harness) => harness.close()));
  activeHarnesses.clear();
});

describe('createOpenPrinterServer', () => {
  it('authenticates a Bearer token and registers only after hello', async () => {
    const connected = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAgentConnected']>>[0]>();
    const authenticateAgent = vi.fn((input: Parameters<OpenPrinterServerOptions<Metadata>['authenticateAgent']>[0]) => {
      expect(input.token).toBe('valid-token');
      expect(input.request.url).toBe('/openprinter/agent');
      expect(input.signal.aborted).toBe(false);
      return {
        agentId: 'agent-1',
        metadata: { organizationId: 'organization-1' },
      };
    });
    const harness = await createHarness({
      authenticateAgent,
      onAgentConnected: connected.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);

    expect(harness.openPrinter.listAgents()).toEqual([]);
    sendAgentMessage(client, createHello());

    const serverHello = await inbox.next('server.hello');
    const event = await connected.promise;

    expect(authenticateAgent).toHaveBeenCalledOnce();
    expect(serverHello.correlationId).toBe('hello-1');
    expect(serverHello.payload.selectedProtocolVersion).toBe(PROTOCOL_VERSION);
    expect(event.agent.agentId).toBe('agent-1');
    expect(event.agent.hello.productId).toBe('oppa');
    expect(harness.openPrinter.listAgents()).toHaveLength(1);
  });

  it('rejects a missing Bearer token before authentication', async () => {
    const authenticationFailed =
      deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAuthenticationFailed']>>[0]>();
    const authenticateAgent = vi.fn();
    const harness = await createHarness({
      authenticateAgent,
      onAuthenticationFailed: authenticationFailed.resolve,
    });
    const client = new WebSocket(`ws://127.0.0.1:${harness.port}/openprinter/agent`);
    harness.clients.add(client);
    const response = await new Promise<number>((resolve, reject) => {
      client.once('unexpected-response', (_request, incoming) => {
        incoming.resume();
        resolve(incoming.statusCode ?? 0);
      });
      client.once('error', reject);
    });

    expect(response).toBe(401);
    expect((await authenticationFailed.promise).reason).toBe('missing-bearer-token');
    expect(authenticateAgent).not.toHaveBeenCalled();
  });

  it('replaces an older connection for the same authenticated agent', async () => {
    const disconnected =
      deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onAgentDisconnected']>>[0]>();
    const harness = await createHarness({
      onAgentDisconnected: (event) => {
        if (event.reason === 'connection-replaced') {
          disconnected.resolve(event);
        }
      },
    });
    const first = await connectClient(harness);
    const firstInbox = new MessageInbox(first);
    sendAgentMessage(first, createHello('hello-first', '0.1.0'));
    await firstInbox.next('server.hello');
    const firstSessionId = harness.openPrinter.getAgent('agent-1')?.sessionId;
    const firstClosed = webSocketCloseCode(first);

    const second = await connectClient(harness);
    const secondInbox = new MessageInbox(second);
    sendAgentMessage(second, createHello('hello-second', '0.2.0'));
    await secondInbox.next('server.hello');

    const event = await disconnected.promise;
    const closeCode = await firstClosed;
    const replacement = harness.openPrinter.getAgent('agent-1');

    expect(event.reason).toBe('connection-replaced');
    expect(closeCode).toBe(4_001);
    expect(replacement?.sessionId).not.toBe(firstSessionId);
    expect(replacement?.hello.agentVersion).toBe('0.2.0');
    expect(harness.openPrinter.listAgents()).toHaveLength(1);
  });

  it('updates inventory and routes each job lifecycle message', async () => {
    const inventoryEvents: unknown[] = [];
    const received = deferred<AgentMessageOf<'agent.job_received'>>();
    const submitted = deferred<AgentMessageOf<'agent.job_submitted'>>();
    const failed = deferred<AgentMessageOf<'agent.job_failed'>>();
    const harness = await createHarness({
      onPrintersChanged: (event) => {
        inventoryEvents.push(event);
      },
      onJobReceived: ({ message }) => received.resolve(message),
      onJobSubmitted: ({ message }) => submitted.resolve(message),
      onJobFailed: ({ message }) => failed.resolve(message),
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');

    sendAgentMessage(client, {
      ...agentEnvelope('inventory-1'),
      type: 'agent.printer_inventory',
      payload: {
        revision: 1,
        printers: [virtualPrinter],
      },
    });
    await eventually(() => {
      expect(harness.openPrinter.getPrinters('agent-1')).toHaveLength(1);
    });

    const updatedPrinter: PrinterDescriptor = {
      ...virtualPrinter,
      name: 'Updated virtual printer',
      availability: 'offline',
    };
    sendAgentMessage(client, {
      ...agentEnvelope('inventory-2'),
      type: 'agent.printer_inventory_changed',
      payload: {
        revision: 2,
        added: [],
        updated: [updatedPrinter],
        removedPrinterIds: [],
      },
    });
    await eventually(() => {
      expect(harness.openPrinter.getPrinters('agent-1')[0]?.name).toBe('Updated virtual printer');
    });

    const delivery = await harness.openPrinter.sendJob('agent-1', printJob);
    expect(delivery.ok).toBe(true);
    const delivered = await inbox.next('server.print_job');
    expect(delivered.payload.jobId).toBe('job-1');

    if (!delivery.ok) {
      throw new Error('Expected the job delivery to succeed.');
    }

    sendAgentMessage(client, {
      ...agentEnvelope('received-1'),
      correlationId: delivery.messageId,
      type: 'agent.job_received',
      payload: {
        jobId: 'job-1',
        idempotencyKey: 'invoice-1',
        status: 'received',
        receivedAt: new Date().toISOString(),
      },
    });
    sendAgentMessage(client, {
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
    });
    sendAgentMessage(client, {
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
    });

    expect((await received.promise).payload.status).toBe('received');
    expect((await submitted.promise).payload.status).toBe('submitted');
    expect((await failed.promise).payload.status).toBe('failed');
    expect(inventoryEvents).toHaveLength(2);
  });

  it('rejects regressing and structurally inconsistent inventory changes', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const harness = await createHarness({
      onProtocolError: protocolError.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');

    sendAgentMessage(client, {
      ...agentEnvelope('inventory-current'),
      type: 'agent.printer_inventory',
      payload: {
        revision: 4,
        printers: [virtualPrinter],
      },
    });
    await eventually(() => {
      expect(harness.openPrinter.getAgent('agent-1')?.printerRevision).toBe(4);
    });

    const closed = webSocketCloseCode(client);
    sendAgentMessage(client, {
      ...agentEnvelope('inventory-stale'),
      type: 'agent.printer_inventory_changed',
      payload: {
        revision: 3,
        added: [],
        updated: [],
        removedPrinterIds: [],
      },
    });

    expect((await protocolError.promise).code).toBe('unexpected-message');
    expect(await closed).toBe(1_002);
  });

  it('returns a structured, throwable offline delivery result', async () => {
    const harness = await createHarness();
    const result = await harness.openPrinter.sendJob('offline-agent', printJob);

    expect(result).toEqual({
      ok: false,
      agentId: 'offline-agent',
      reason: 'agent-offline',
      retryable: true,
    });
    expect(() => deliveryResultOrThrow(result)).toThrowError(OpenPrinterDeliveryError);
  });

  it('reports invalid inbound payloads without exposing raw content', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const harness = await createHarness({
      onProtocolError: protocolError.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');
    const closed = webSocketCloseCode(client);

    client.send(
      JSON.stringify({
        ...agentEnvelope('bad-1'),
        type: 'agent.job_received',
        payload: {
          privateReceipt: 'must-not-appear-in-errors',
        },
      }),
    );

    const event = await protocolError.promise;
    const closeCode = await closed;

    expect(event.code).toBe('invalid-message');
    expect(event.error.message).not.toContain('must-not-appear-in-errors');
    expect(closeCode).toBe(1_002);
    expect(harness.openPrinter.getAgent('agent-1')).toBeNull();
  });

  it('distinguishes unsupported protocol versions', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const harness = await createHarness({
      onProtocolError: protocolError.resolve,
    });
    const client = await connectClient(harness);

    client.send(
      JSON.stringify({
        ...createHello(),
        protocolVersion: 99,
      }),
    );

    expect((await protocolError.promise).code).toBe('unsupported-protocol-version');
  });

  it('rejects a hello identity that differs from the Bearer token', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const harness = await createHarness({
      onProtocolError: protocolError.resolve,
    });
    const client = await connectClient(harness);
    const hello = createHello();

    sendAgentMessage(client, {
      ...hello,
      payload: {
        ...hello.payload,
        agentId: 'different-agent',
      },
    });

    expect((await protocolError.promise).code).toBe('identity-mismatch');
    expect(harness.openPrinter.listAgents()).toEqual([]);
  });

  it('enforces the configured inbound message-size limit', async () => {
    const protocolError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onProtocolError']>>[0]>();
    const harness = await createHarness({
      maxMessageBytes: 1_024,
      onProtocolError: protocolError.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');

    client.send('x'.repeat(1_025));

    expect((await protocolError.promise).code).toBe('message-too-large');
  });

  it('enforces the configured outbound message-size limit while offline', async () => {
    const harness = await createHarness({
      maxMessageBytes: 1_024,
    });
    const largeJob: PrintJob = {
      ...printJob,
      document: {
        width: 80,
        sections: [
          {
            type: 'text',
            value: 'x'.repeat(2_000),
          },
        ],
      },
    };

    await expect(harness.openPrinter.sendJob('offline-agent', largeJob)).rejects.toMatchObject({
      code: 'message_too_large',
    });
  });

  it('expires a session that does not answer heartbeats', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout'],
    });
    const timedOut = deferred<void>();
    const disconnected = deferred<void>();
    const harness = await createHarness({
      heartbeatIntervalMs: 5_000,
      heartbeatTimeoutMs: 6_000,
      onHeartbeatTimeout: () => timedOut.resolve(),
      onAgentDisconnected: ({ reason }) => {
        if (reason === 'heartbeat-timeout') {
          disconnected.resolve();
        }
      },
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');
    await inbox.next('server.heartbeat');

    await vi.advanceTimersByTimeAsync(6_001);
    await timedOut.promise;
    await disconnected.promise;

    expect(harness.openPrinter.getAgent('agent-1')).toBeNull();
  });

  it('isolates host callback failures from protocol routing', async () => {
    const callbackError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onCallbackError']>>[0]>();
    const harness = await createHarness({
      onJobReceived: () => {
        throw new Error('host database unavailable');
      },
      onCallbackError: callbackError.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');

    sendAgentMessage(client, {
      ...agentEnvelope('received-callback-error'),
      correlationId: 'delivery-1',
      type: 'agent.job_received',
      payload: {
        jobId: 'job-1',
        idempotencyKey: 'invoice-1',
        status: 'received',
        receivedAt: new Date().toISOString(),
      },
    });

    expect((await callbackError.promise).callback).toBe('onJobReceived');
    expect(harness.openPrinter.getAgent('agent-1')).not.toBeNull();
  });

  it('bounds a stalled lifecycle callback and continues routing later messages', async () => {
    const callbackError = deferred<Parameters<NonNullable<OpenPrinterServerOptions<Metadata>['onCallbackError']>>[0]>();
    const inventory = deferred<void>();
    const harness = await createHarness({
      callbackTimeoutMs: 20,
      onJobReceived: () => new Promise<void>(() => undefined),
      onPrintersChanged: () => inventory.resolve(),
      onCallbackError: callbackError.resolve,
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');

    sendAgentMessage(client, {
      ...agentEnvelope('received-stalled-callback'),
      correlationId: 'delivery-1',
      type: 'agent.job_received',
      payload: {
        jobId: 'job-1',
        idempotencyKey: 'invoice-1',
        status: 'received',
        receivedAt: new Date().toISOString(),
      },
    });
    sendAgentMessage(client, {
      ...agentEnvelope('inventory-after-stall'),
      type: 'agent.printer_inventory',
      payload: {
        revision: 1,
        printers: [virtualPrinter],
      },
    });

    const event = await callbackError.promise;
    await inventory.promise;
    expect(event.callback).toBe('onJobReceived');
    expect(event.error).toMatchObject({ name: 'HostCallbackTimeoutError' });
    expect(harness.openPrinter.getPrinters('agent-1')).toHaveLength(1);
  });

  it('closes protocol violations before a stalled notification callback', async () => {
    const harness = await createHarness({
      callbackTimeoutMs: 500,
      onProtocolError: () => new Promise<void>(() => undefined),
    });
    const client = await connectClient(harness);
    const inbox = new MessageInbox(client);
    sendAgentMessage(client, createHello());
    await inbox.next('server.hello');
    const closed = webSocketCloseCode(client);

    client.send('{not-json');

    await expect(
      Promise.race([
        closed,
        new Promise<never>((_resolve, reject) => {
          setTimeout(() => reject(new Error('socket close was blocked by callback')), 250);
        }),
      ]),
    ).resolves.toBe(1_002);
  });

  it('rejects an unauthorized upgrade before a stalled audit callback', async () => {
    const harness = await createHarness({
      callbackTimeoutMs: 500,
      onAuthenticationFailed: () => new Promise<void>(() => undefined),
    });
    const client = new WebSocket(`ws://127.0.0.1:${harness.port}/openprinter/agent`);
    harness.clients.add(client);
    const response = new Promise<number>((resolve, reject) => {
      client.once('unexpected-response', (_request, incoming) => {
        incoming.resume();
        resolve(incoming.statusCode ?? 0);
      });
      client.once('error', reject);
    });

    await expect(
      Promise.race([
        response,
        new Promise<never>((_resolve, reject) => {
          setTimeout(() => reject(new Error('HTTP rejection was blocked by callback')), 250);
        }),
      ]),
    ).resolves.toBe(401);
  });
});

async function createHarness(overrides: Partial<OpenPrinterServerOptions<Metadata>> = {}): Promise<Harness> {
  const httpServer = createServer((_request, response) => {
    response.writeHead(404).end();
  });
  const openPrinter = createOpenPrinterServer<Metadata>({
    path: '/openprinter/agent',
    authenticateAgent: ({ token }) =>
      token === 'valid-token'
        ? {
            agentId: 'agent-1',
            metadata: { organizationId: 'organization-1' },
          }
        : null,
    ...overrides,
  });
  httpServer.on('upgrade', openPrinter.handleUpgrade);
  httpServer.listen(0, '127.0.0.1');
  await once(httpServer, 'listening');
  const address = httpServer.address();

  if (address === null || typeof address === 'string') {
    throw new Error('Expected a loopback TCP listener.');
  }

  const clients = new Set<WebSocket>();
  const harness: Harness = {
    httpServer,
    openPrinter,
    port: address.port,
    clients,
    async close() {
      if (!activeHarnesses.delete(harness)) {
        return;
      }

      await openPrinter.close();
      for (const client of clients) {
        if (client.readyState !== WebSocket.CLOSED) {
          client.terminate();
        }
      }
      httpServer.closeAllConnections();
      if (httpServer.listening) {
        await new Promise<void>((resolve) => {
          httpServer.close(() => resolve());
        });
      }
    },
  };
  activeHarnesses.add(harness);
  return harness;
}

async function connectClient(harness: Harness): Promise<WebSocket> {
  const client = new WebSocket(`ws://127.0.0.1:${harness.port}/openprinter/agent`, {
    headers: {
      authorization: 'Bearer valid-token',
    },
  });
  harness.clients.add(client);
  await once(client, 'open');
  return client;
}

function webSocketCloseCode(socket: WebSocket): Promise<number> {
  return new Promise((resolve) => {
    socket.once('close', (code) => {
      resolve(code);
    });
  });
}

class MessageInbox {
  readonly #messages: ServerMessage[] = [];
  readonly #waiters: Array<(message: ServerMessage) => void> = [];

  public constructor(socket: WebSocket) {
    socket.on('message', (data, isBinary) => {
      if (isBinary) {
        return;
      }

      const message = decodeServerMessage(Buffer.isBuffer(data) ? data : Buffer.from(data as ArrayBuffer));
      const waiter = this.#waiters.shift();

      if (waiter === undefined) {
        this.#messages.push(message);
      } else {
        waiter(message);
      }
    });
  }

  public async next<Type extends ServerMessage['type']>(
    type: Type,
  ): Promise<Extract<ServerMessage, { readonly type: Type }>> {
    for (;;) {
      const existingIndex = this.#messages.findIndex((message) => message.type === type);
      if (existingIndex >= 0) {
        return this.#messages.splice(existingIndex, 1)[0] as Extract<ServerMessage, { readonly type: Type }>;
      }

      const message = await new Promise<ServerMessage>((resolve) => {
        this.#waiters.push(resolve);
      });

      if (message.type === type) {
        return message as Extract<ServerMessage, { readonly type: Type }>;
      }

      this.#messages.push(message);
    }
  }
}

const virtualPrinter: PrinterDescriptor = {
  id: 'printer-1',
  fingerprint: 'virtual:printer-1',
  name: 'Virtual printer',
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
    sections: [
      {
        type: 'text',
        value: 'Test receipt',
        align: 'center',
        bold: true,
      },
      { type: 'cut' },
    ],
  },
};

function createHello(messageId = 'hello-1', agentVersion = '0.1.0'): AgentMessageOf<'agent.hello'> {
  return {
    ...agentEnvelope(messageId),
    type: 'agent.hello',
    payload: {
      agentId: 'agent-1',
      agentVersion,
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

function sendAgentMessage(socket: WebSocket, message: AgentMessage): void {
  socket.send(encodeAgentMessage(message));
}

function deferred<Value>(): {
  readonly promise: Promise<Value>;
  readonly resolve: (value: Value) => void;
} {
  let resolvePromise!: (value: Value) => void;
  const promise = new Promise<Value>((resolve) => {
    resolvePromise = resolve;
  });
  return {
    promise,
    resolve: resolvePromise,
  };
}

async function eventually(assertion: () => void): Promise<void> {
  let lastError: unknown;

  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      assertion();
      return;
    } catch (error) {
      lastError = error;
      await new Promise<void>((resolve) => {
        setImmediate(resolve);
      });
    }
  }

  throw lastError;
}
