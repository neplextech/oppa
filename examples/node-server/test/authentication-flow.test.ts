import { generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { createServer } from 'node:http';

import { decodeBase64Url, PROTOCOL_VERSION, type PrintJob } from '@openprinter/protocol';
import { createOpenPrinterServer, type OpenPrinterSession } from '@openprinter/server';
import { afterEach, describe, expect, it } from 'vitest';
import { WebSocket, WebSocketServer, type RawData } from 'ws';

const sockets: WebSocket[] = [];
const readers = new WeakMap<WebSocket, JsonMessageReader>();

afterEach(() => {
  for (const socket of sockets.splice(0)) socket.terminate();
});

describe('pairing and challenge integration', () => {
  it('pairs, exchanges a job, and reconnects with the same credential', async () => {
    let connected: OpenPrinterSession<unknown> | null = null;
    let connectedResolve: (() => void) | null = null;
    let jobReceivedResolve: (() => void) | null = null;
    const openprinter = createOpenPrinterServer({
      brand: { name: 'Integration Server' },
      serverId: 'integration-server',
      heartbeatIntervalMs: 60_000,
      heartbeatTimeoutMs: 120_000,
      onAgentConnected: ({ session }) => {
        connected = session;
        connectedResolve?.();
      },
      onJobReceived: () => jobReceivedResolve?.(),
    });
    const wss = new WebSocketServer({ noServer: true });
    const http = createServer((request, response) => {
      void (async () => {
        if (request.method === 'GET' && request.url === openprinter.paths.discovery) {
          response.setHeader('content-type', 'application/json');
          response.end(JSON.stringify(await openprinter.discover()));
          return;
        }
        if (request.method === 'POST' && request.url === openprinter.paths.pairing) {
          const chunks: Uint8Array[] = [];
          for await (const chunk of request) {
            if (typeof chunk === 'string') chunks.push(Buffer.from(chunk));
            else if (chunk instanceof Uint8Array) chunks.push(chunk);
          }
          const result = await openprinter.pair(JSON.parse(Buffer.concat(chunks).toString('utf8')));
          response.setHeader('content-type', 'application/json');
          response.end(JSON.stringify(result));
          return;
        }
        response.statusCode = 404;
        response.end();
      })();
    });
    http.on('upgrade', (request, socket, head) => {
      if (request.url !== openprinter.paths.gateway) return socket.destroy();
      wss.handleUpgrade(request, socket, head, (webSocket) => {
        openprinter.handleGatewayConnection(webSocket);
      });
    });
    await new Promise<void>((resolve) => http.listen(0, '127.0.0.1', resolve));
    const address = http.address();
    if (address === null || typeof address === 'string') throw new Error('Missing test address.');
    const baseUrl = `http://127.0.0.1:${address.port}`;

    try {
      const discovery = await fetch(`${baseUrl}${openprinter.paths.discovery}`).then(async (response) =>
        response.json(),
      );
      expect(discovery).toMatchObject({ server: { id: 'integration-server' } });

      const keys = generateKeyPairSync('ed25519');
      const publicJwk = keys.publicKey.export({ format: 'jwk' });
      const pairingCode = await openprinter.createPairingCode();
      const paired = await fetch(`${baseUrl}${openprinter.paths.pairing}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          protocolVersion: PROTOCOL_VERSION,
          code: pairingCode.code,
          agent: {
            name: 'Integration Agent',
            version: '1.0.0',
            platform: 'test',
            installationId: 'integration-installation',
          },
          credential: {
            algorithm: 'Ed25519',
            publicKey: { kty: 'OKP', crv: 'Ed25519', x: publicJwk.x },
          },
        }),
      }).then(async (response) => response.json() as Promise<{ agentId: string; keyId: string }>);

      const first = await connectAuthenticated(
        `${baseUrl.replace('http:', 'ws:')}${openprinter.paths.gateway}`,
        paired,
        keys.privateKey,
      );
      sockets.push(first);
      const firstHello = envelope('agent.hello', {
        agentId: paired.agentId,
        agentVersion: '1.0.0',
        productId: 'oppa',
        productVersion: '1.0.0',
        supportedProtocolVersions: [PROTOCOL_VERSION],
      });
      const firstConnected = new Promise<void>((resolve) => (connectedResolve = resolve));
      first.send(JSON.stringify(firstHello));
      await firstConnected;
      expect((await receiveJson(first)).type).toBe('server.hello');
      expect((await receiveJson(first)).type).toBe('server.heartbeat');

      first.send(
        JSON.stringify(
          envelope('agent.printer_inventory', {
            revision: 1,
            printers: [
              {
                id: 'printer-01',
                fingerprint: 'virtual:integration',
                name: 'Virtual receipt',
                kind: 'virtual',
                connection: { type: 'virtual' },
                enabled: true,
                availability: 'online',
              },
            ],
          }),
        ),
      );
      const job: PrintJob = {
        jobId: 'job-01',
        idempotencyKey: 'job-01-v1',
        printerId: 'printer-01',
        document: { width: 80, sections: [{ type: 'text', value: 'Integration test' }] },
        createdAt: '2026-08-01T09:00:00.000Z',
      };
      const received = new Promise<void>((resolve) => (jobReceivedResolve = resolve));
      const delivery = await connected!.sendJob(job);
      expect(delivery.ok).toBe(true);
      const printMessage = await receiveJson(first);
      expect(printMessage.type).toBe('server.print_job');
      const acknowledgement = {
        ...envelope('agent.job_received', {
          jobId: 'job-01',
          idempotencyKey: 'job-01-v1',
          status: 'received',
          receivedAt: '2026-08-01T09:00:01.000Z',
        }),
        correlationId: printMessage.messageId,
      };
      first.send(JSON.stringify(acknowledgement));
      await received;
      first.close();

      const second = await connectAuthenticated(
        `${baseUrl.replace('http:', 'ws:')}${openprinter.paths.gateway}`,
        paired,
        keys.privateKey,
      );
      sockets.push(second);
      const reconnected = new Promise<void>((resolve) => (connectedResolve = resolve));
      second.send(JSON.stringify({ ...firstHello, messageId: 'hello-02' }));
      await reconnected;
      expect((await receiveJson(second)).type).toBe('server.hello');
    } finally {
      for (const socket of sockets.splice(0)) socket.terminate();
      wss.close();
      await new Promise<void>((resolve) => http.close(() => resolve()));
    }
  });
});

async function connectAuthenticated(
  url: string,
  paired: { agentId: string; keyId: string },
  privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'],
): Promise<WebSocket> {
  const socket = new WebSocket(url);
  readers.set(socket, new JsonMessageReader(socket));
  await new Promise<void>((resolve, reject) => {
    socket.once('open', resolve);
    socket.once('error', reject);
  });
  const challenge = await receiveJson(socket);
  if (typeof challenge.challengeId !== 'string' || typeof challenge.payload !== 'string') {
    throw new Error('Server returned an invalid authentication challenge.');
  }
  await sendText(
    socket,
    JSON.stringify({
      type: 'auth.response',
      challengeId: challenge.challengeId,
      agentId: paired.agentId,
      keyId: paired.keyId,
      algorithm: 'Ed25519',
      signature: sign(null, decodeBase64Url(challenge.payload), privateKey).toString('base64url'),
    }),
  );
  expect((await receiveJson(socket)).type).toBe('auth.accepted');
  return socket;
}

function sendText(socket: WebSocket, value: string): Promise<void> {
  return new Promise((resolve, reject) => {
    socket.send(value, (error) => {
      if (error == null) resolve();
      else reject(error);
    });
  });
}

type JsonObject = Record<string, unknown>;

function receiveJson(socket: WebSocket): Promise<JsonObject> {
  const reader = readers.get(socket);
  if (reader === undefined) throw new Error('Socket does not have a JSON reader.');
  return reader.next();
}

class JsonMessageReader {
  readonly #queued: JsonObject[] = [];
  readonly #waiting: Array<{ resolve(value: JsonObject): void; reject(error: Error): void }> = [];

  constructor(socket: WebSocket) {
    socket.on('message', (data) => {
      const value = JSON.parse(rawDataText(data)) as JsonObject;
      const waiter = this.#waiting.shift();
      if (waiter === undefined) this.#queued.push(value);
      else waiter.resolve(value);
    });
    socket.on('error', (error) => {
      for (const waiter of this.#waiting.splice(0)) waiter.reject(error);
    });
  }

  next(): Promise<JsonObject> {
    const value = this.#queued.shift();
    if (value !== undefined) return Promise.resolve(value);
    return new Promise((resolve, reject) => this.#waiting.push({ resolve, reject }));
  }
}

function rawDataText(data: RawData): string {
  if (data instanceof ArrayBuffer) return Buffer.from(data).toString('utf8');
  if (Array.isArray(data)) return Buffer.concat(data).toString('utf8');
  return data.toString('utf8');
}

function envelope(type: string, payload: unknown) {
  return {
    protocolVersion: PROTOCOL_VERSION,
    messageId: `message-${randomUUID()}`,
    sentAt: '2026-08-01T09:00:00.000Z',
    type,
    payload,
  };
}
