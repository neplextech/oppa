import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import type { Duplex } from 'node:stream';

import { MAX_WIRE_MESSAGE_BYTES, parsePrintJob } from '@openprinter/protocol';
import { createOpenPrinterServer, type OpenPrinterSession } from '@openprinter/server';
import { WebSocketServer } from 'ws';

import { DEV_UI_HTML } from './dev-ui.js';
import { HttpError, readJson, sendHtml, sendJson } from './http.js';

const HOST = '127.0.0.1';
const PORT = parsePort(process.env.PORT);
const MAX_PAIRING_REQUEST_BYTES = 16 * 1024;
const MAX_JOB_REQUEST_BYTES = 2 * 1024 * 1024;

interface PairingMetadata {
  readonly tenantId: string;
}

const sessions = new Map<string, OpenPrinterSession<PairingMetadata>>();
const openprinter = createOpenPrinterServer<PairingMetadata>({
  brand: { name: 'OpenPrinter Node.js example' },
  serverId: 'oppa-node-example',
  serverVersion: '1.0.0',
  onAgentConnected: ({ agent, session }) => {
    sessions.set(agent.agentId, session);
    log('agent.connected', { agentId: agent.agentId, productId: agent.hello.productId });
  },
  onAgentDisconnected: ({ agent, session, reason }) => {
    if (sessions.get(agent.agentId) === session) sessions.delete(agent.agentId);
    log('agent.disconnected', { agentId: agent.agentId, reason });
  },
  onPrintersChanged: ({ agent, revision, printers }) => {
    log('agent.printers', { agentId: agent.agentId, revision, printerCount: printers.length });
  },
  onJobReceived: ({ agent, message }) => {
    log('job.received', { agentId: agent.agentId, jobId: message.payload.jobId });
  },
  onJobSubmitted: ({ agent, message }) => {
    log('job.submitted', { agentId: agent.agentId, jobId: message.payload.jobId });
  },
  onJobFailed: ({ agent, message }) => {
    log('job.failed', { agentId: agent.agentId, jobId: message.payload.jobId, code: message.payload.error.code });
  },
});

const webSockets = new WebSocketServer({
  noServer: true,
  clientTracking: false,
  perMessageDeflate: false,
  maxPayload: MAX_WIRE_MESSAGE_BYTES,
});

const http = createServer((request, response) => {
  void route(request, response).catch((error: unknown) => handleHttpError(response, error));
});

http.on('upgrade', (request, socket, head) => handleUpgrade(request, socket, head));

http.listen(PORT, HOST, () => {
  logBanner();
  void createDevelopmentPairingCode();
});

async function route(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const method = request.method ?? 'GET';
  const url = new URL(request.url ?? '/', `http://${HOST}:${PORT}`);

  if (method === 'GET' && url.pathname === openprinter.paths.discovery) {
    sendJson(response, 200, await openprinter.discover());
    return;
  }
  if (method === 'POST' && url.pathname === openprinter.paths.pairing) {
    const result = await openprinter.pair(await readJson(request, MAX_PAIRING_REQUEST_BYTES), {
      ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
    });
    sendJson(response, 'error' in result ? pairingStatus(result.error.code) : 200, result);
    return;
  }
  if (method === 'POST' && url.pathname === '/development/pairing-code') {
    const pairing = await openprinter.createPairingCode({ metadata: { tenantId: 'local-development' } });
    sendJson(response, 201, { code: pairing.code, expiresAt: pairing.expiresAt.toISOString() });
    return;
  }
  if (method === 'GET' && url.pathname === '/agents') {
    const agents = [...sessions.values()].flatMap((s) => {
      const agent = s.getAgent();
      return agent ? [{ agentId: agent.agentId, productId: agent.hello.productId, agentVersion: agent.hello.agentVersion, connectedAt: agent.connectedAt, lastSeenAt: agent.lastSeenAt }] : [];
    });
    sendJson(response, 200, agents);
    return;
  }
  const agentPrintersMatch = /^\/agents\/([^/]+)\/printers$/.exec(url.pathname);
  if (method === 'GET' && agentPrintersMatch) {
    const agentId = decodeURIComponent(agentPrintersMatch[1]!);
    const session = sessions.get(agentId);
    if (session === undefined) {
      sendJson(response, 409, { error: { code: 'agent_offline', message: 'The agent is not connected.' } });
      return;
    }
    sendJson(response, 200, session.getPrinters());
    return;
  }
  const jobMatch = /^\/agents\/([^/]+)\/jobs$/.exec(url.pathname);
  if (method === 'POST' && jobMatch) {
    const agentId = decodeURIComponent(jobMatch[1]!);
    const session = sessions.get(agentId);
    if (session === undefined) {
      sendJson(response, 409, { error: { code: 'agent_offline', message: 'The agent is not connected.' } });
      return;
    }
    const job = parsePrintJob(await readJson(request, MAX_JOB_REQUEST_BYTES));
    const result = await session.sendJob(job);
    sendJson(response, result.ok ? 202 : 409, result);
    return;
  }
  const testPrintMatch = /^\/development\/test-print\/([^/]+)\/([^/]+)$/.exec(url.pathname);
  if (method === 'POST' && testPrintMatch) {
    const agentId = decodeURIComponent(testPrintMatch[1]!);
    const printerId = decodeURIComponent(testPrintMatch[2]!);
    const session = sessions.get(agentId);
    if (session === undefined) {
      sendJson(response, 409, { error: { code: 'agent_offline', message: 'The agent is not connected.' } });
      return;
    }
    const printers = session.getPrinters();
    const printer = printers.find((p) => p.fingerprint === printerId);
    const widths = printer?.capabilities?.mediaWidths;
    const width: 58 | 80 = widths?.includes(80) ? 80 : 58;
    const now = new Date();
    const job = parsePrintJob({
      jobId: `job_test_${Date.now()}`,
      idempotencyKey: `test_${Math.random().toString(36).slice(2)}`,
      printerId,
      createdAt: now.toISOString(),
      document: {
        width,
        sections: [
          { type: 'text', value: 'OpenPrinter Test', bold: true, align: 'center' },
          { type: 'divider' },
          { type: 'text', value: `Agent: ${agentId}`, align: 'center' },
          { type: 'text', value: now.toLocaleString(), align: 'center' },
          { type: 'feed', lines: 3 },
          { type: 'cut' },
        ],
      },
    });
    const result = await session.sendJob(job);
    sendJson(response, result.ok ? 202 : 409, result);
    return;
  }
  if (method === 'GET' && url.pathname === '/dev') {
    sendHtml(response, 200, DEV_UI_HTML);
    return;
  }
  if (method === 'GET' && url.pathname === '/') {
    sendJson(response, 200, {
      name: 'OpenPrinter Node.js example',
      devUi: `http://${HOST}:${PORT}/dev`,
      routes: {
        discovery: `GET ${openprinter.paths.discovery}`,
        pairing: `POST ${openprinter.paths.pairing}`,
        createDevelopmentPairingCode: 'POST /development/pairing-code',
        testPrint: 'POST /development/test-print/:agentId/:printerId',
        agents: 'GET /agents',
        agentPrinters: 'GET /agents/:agentId/printers',
        sendJob: 'POST /agents/:agentId/jobs',
      },
      connectedAgents: [...sessions.keys()],
    });
    return;
  }
  sendJson(response, 404, { error: { code: 'not_found', message: 'Route not found.' } });
}

function handleUpgrade(request: IncomingMessage, socket: Duplex, head: Buffer): void {
  const pathname = new URL(request.url ?? '/', `http://${HOST}:${PORT}`).pathname;
  if (pathname !== openprinter.paths.gateway) {
    socket.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n');
    return;
  }
  webSockets.handleUpgrade(request, socket, head, (webSocket) => {
    openprinter.handleGatewayConnection(webSocket);
  });
}

async function createDevelopmentPairingCode(): Promise<void> {
  const pairing = await openprinter.createPairingCode({ metadata: { tenantId: 'local-development' } });
  const serverUrl = `http://${HOST}:${PORT}/`;
  const encoded = Buffer.from(serverUrl).toString('base64url');
  const deepLink = `oppa://pair?server=${encoded}&key=${pairing.code}`;
  const c = ansi;
  process.stdout.write(
    `\n  ${c.bold}${c.yellow}Pairing code${c.reset}  ${c.bold}${pairing.code}${c.reset}` +
    `  ${c.dim}(expires ${pairing.expiresAt.toLocaleTimeString()})${c.reset}\n` +
    `  ${c.dim}Deep link${c.reset}     ${c.green}${deepLink}${c.reset}\n\n`,
  );
}

function pairingStatus(code: string): number {
  if (code === 'pairing_rate_limited') return 429;
  if (code === 'server_error') return 500;
  return 400;
}

function handleHttpError(response: ServerResponse, error: unknown): void {
  if (error instanceof HttpError) {
    sendJson(response, error.status, { error: { code: error.code, message: error.message } });
    return;
  }
  log('http.error', { error: error instanceof Error ? error.message : 'unknown error' });
  sendJson(response, 500, { error: { code: 'server_error', message: 'The request could not be completed.' } });
}

function parsePort(value: string | undefined): number {
  const port = value === undefined ? 8787 : Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error('PORT must be between 1 and 65535.');
  return port;
}

const ansi = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  cyan: '\x1b[36m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
};

function logBanner(): void {
  const base = `http://${HOST}:${PORT}`;
  const c = ansi;
  const row = (label: string, value: string): string =>
    `  ${c.dim}${label.padEnd(12)}${c.reset}${c.cyan}${value}${c.reset}\n`;
  process.stdout.write(
    `\n${c.bold}  OpenPrinter Node.js Example${c.reset}\n` +
    `  ${'─'.repeat(44)}\n` +
    row('Server', base) +
    row('Dev UI', `${base}/dev`) +
    row('Discovery', `${base}${openprinter.paths.discovery}`) +
    row('Pairing', `${base}${openprinter.paths.pairing}`) +
    row('Gateway', `${base.replace('http', 'ws')}${openprinter.paths.gateway}`) +
    `\n  ${c.dim}⚠  Volatile in-memory stores. Development only.${c.reset}\n`,
  );
}

function log(event: string, details: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify({ timestamp: new Date().toISOString(), event, ...details })}\n`);
}
