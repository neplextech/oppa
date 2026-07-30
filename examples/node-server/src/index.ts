import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import type { Duplex } from 'node:stream';

import { MAX_WIRE_MESSAGE_BYTES, parsePrintJob, ProtocolError } from '@openprinter/protocol';
import {
  createOpenPrinterServer,
  type DeliveryFailure,
  type OpenPrinterSession,
  type OpenPrinterTransportCloseRequest,
} from '@openprinter/server';
import { WebSocket, WebSocketServer, type RawData } from 'ws';

import { DevelopmentAuthError, DevelopmentAuthStore } from './development-auth.js';
import { escapeHtml, HttpError, readForm, readJson, sendHtml, sendJson } from './http.js';
import { EXAMPLE_CLIENT_ID } from './product-config.js';

const HOST = '127.0.0.1';
const PORT = parsePort(process.env.PORT);
const MAX_FORM_BYTES = 16 * 1_024;
const MAX_JOB_REQUEST_BYTES = 2 * 1_024 * 1_024;

const authorization = new DevelopmentAuthStore({
  clientId: EXAMPLE_CLIENT_ID,
});

interface AgentSessionMetadata {
  readonly issuedAt: string;
  readonly expiresAt: string;
}

const localSessions = new Map<string, OpenPrinterSession<AgentSessionMetadata>>();
const acceptedSessions = new Set<OpenPrinterSession<AgentSessionMetadata>>();
const webSocketServer = new WebSocketServer({
  clientTracking: false,
  maxPayload: MAX_WIRE_MESSAGE_BYTES,
  noServer: true,
  perMessageDeflate: false,
});

const openPrinter = createOpenPrinterServer<AgentSessionMetadata>({
  brand: {
    name: 'OpenPrinter Node.js example',
  },
  serverId: 'oppa-node-example',
  serverVersion: '0.1.0',
  onAgentConnected: ({ agent, session }) => {
    const previous = localSessions.get(agent.agentId);
    localSessions.set(agent.agentId, session);
    if (previous !== undefined && previous !== session) {
      void previous.disconnect({
        code: 'connection_replaced',
        reason: 'A newer connection replaced this local example session.',
        reconnect: false,
      });
    }

    logEvent('agent.connected', {
      agentId: agent.agentId,
      productId: agent.hello.productId,
      agentVersion: agent.hello.agentVersion,
    });
  },
  onAgentDisconnected: ({ agent, session, reason }) => {
    if (localSessions.get(agent.agentId) === session) {
      localSessions.delete(agent.agentId);
    }

    logEvent('agent.disconnected', {
      agentId: agent.agentId,
      reason,
    });
  },
  onAuthenticationMetadata: ({ agent, message }) => {
    logEvent('agent.authentication_metadata', {
      agentId: agent.agentId,
      method: message.payload.method,
      hasSubject: message.payload.subject !== undefined,
    });
  },
  onPrintersChanged: ({ agent, kind, revision, printers }) => {
    logEvent('agent.printers_changed', {
      agentId: agent.agentId,
      kind,
      revision,
      printerCount: printers.length,
    });
  },
  onJobReceived: ({ agent, message }) => {
    logEvent('job.received', {
      agentId: agent.agentId,
      jobId: message.payload.jobId,
      idempotencyKey: message.payload.idempotencyKey,
    });
  },
  onJobSubmitted: ({ agent, message }) => {
    logEvent('job.submitted', {
      agentId: agent.agentId,
      jobId: message.payload.jobId,
      printerId: message.payload.printerId,
    });
  },
  onJobFailed: ({ agent, message }) => {
    logEvent('job.failed', {
      agentId: agent.agentId,
      jobId: message.payload.jobId,
      errorCode: message.payload.error.code,
      retryable: message.payload.error.retryable,
    });
  },
  onDiagnostics: ({ agent, message }) => {
    logEvent('agent.diagnostics', {
      agentId: agent.agentId,
      health: message.payload.health,
      issueCount: message.payload.issues.length,
      printersOnline: message.payload.printersOnline,
      printersTotal: message.payload.printersTotal,
    });
  },
  onHeartbeatTimeout: ({ agent, lastHeartbeatAt }) => {
    logEvent('agent.heartbeat_timeout', {
      agentId: agent.agentId,
      lastHeartbeatAt,
    });
  },
  onProtocolError: ({ agentId, code, error }) => {
    logEvent('agent.protocol_error', {
      agentId,
      code,
      errorName: error.name,
      message: error.message,
    });
  },
  onCallbackError: ({ callback, error }) => {
    logEvent('host.callback_error', {
      callback,
      errorName: error instanceof Error ? error.name : 'NonErrorThrownValue',
    });
  },
});

const httpServer = createServer((request, response) => {
  void handleRequest(request, response).catch((error: unknown) => {
    handleHttpError(response, error);
  });
});

httpServer.on('upgrade', handleOpenPrinterUpgrade);
httpServer.on('clientError', (error, socket) => {
  logEvent('http.client_error', {
    code: 'code' in error && typeof error.code === 'string' ? error.code : 'HTTP_CLIENT_ERROR',
    errorName: error.name,
  });

  if (!socket.destroyed) {
    socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n');
  }
});

httpServer.listen(PORT, HOST, () => {
  logEvent('server.listening', {
    authorizationUrl: `http://${HOST}:${PORT}/authorize`,
    gatewayUrl: `ws://${HOST}:${PORT}/openprinter/agent`,
    tokenUrl: `http://${HOST}:${PORT}/token`,
  });
});

let shutdownStarted = false;
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => {
    void shutdown(signal);
  });
}

function handleOpenPrinterUpgrade(request: IncomingMessage, socket: Duplex, head: Buffer): void {
  let pathname: string;
  try {
    pathname = new URL(request.url ?? '/', `http://${HOST}:${PORT}`).pathname;
  } catch {
    rejectUpgrade(socket, 400, 'Bad Request');
    return;
  }

  if (pathname !== '/openprinter/agent') {
    rejectUpgrade(socket, 404, 'Not Found');
    return;
  }

  const token = bearerToken(request.headers.authorization);
  if (token === null) {
    logEvent('agent.authentication_failed', {
      reason: 'missing-or-malformed-bearer-token',
      ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
    });
    rejectUpgrade(socket, 401, 'Unauthorized');
    return;
  }

  const identity = authorization.authenticateAccessToken(token);
  if (identity === null) {
    logEvent('agent.authentication_failed', {
      reason: 'rejected',
      ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
    });
    rejectUpgrade(socket, 401, 'Unauthorized');
    return;
  }

  try {
    webSocketServer.handleUpgrade(request, socket, head, (webSocket) => {
      const session = openPrinter.accept({
        identity: {
          agentId: identity.agentId,
          metadata: {
            issuedAt: identity.issuedAt,
            expiresAt: identity.expiresAt,
          },
        },
        transport: {
          send: (message) => sendWebSocketMessage(webSocket, message),
          close: (close) => closeWebSocket(webSocket, close),
        },
      });
      acceptedSessions.add(session);

      webSocket.on('message', (data: RawData, isBinary: boolean) => {
        if (isBinary) {
          webSocket.close(1_003, 'Text messages required');
          void session.transportClosed({
            reason: 'transport-error',
            detail: 'The WebSocket host rejected a binary frame.',
          });
          return;
        }

        void session.receive(rawDataToUtf8(data));
      });
      webSocket.on('close', (_code: number, reason: Buffer) => {
        acceptedSessions.delete(session);
        void session.transportClosed({
          reason: 'peer-closed',
          ...(reason.length === 0 ? {} : { detail: reason.toString('utf8') }),
        });
      });
      webSocket.on('error', (error: Error) => {
        void session.transportClosed({
          reason: 'transport-error',
          detail: error.name,
        });
      });
    });
  } catch {
    rejectUpgrade(socket, 400, 'Bad Request');
  }
}

async function handleRequest(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const method = request.method ?? 'GET';
  const url = new URL(request.url ?? '/', `http://${HOST}:${PORT}`);

  if (method === 'GET' && url.pathname === '/') {
    const acceptsHtml = (request.headers['accept'] ?? '').includes('text/html');
    if (acceptsHtml) {
      sendHtml(response, 200, dashboardPage(HOST, PORT), {
        'content-security-policy':
          "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'self'",
      });
    } else {
      sendJson(response, 200, {
        name: 'OpenPrinter Node.js development example',
        developmentOnly: true,
        endpoints: {
          authorize: 'GET /authorize',
          token: 'POST /token',
          agents: 'GET /agents',
          printers: 'GET /agents/:agentId/printers',
          jobs: 'POST /agents/:agentId/jobs',
          gateway: 'WS /openprinter/agent',
        },
      });
    }
    return;
  }

  if (method === 'GET' && url.pathname === '/authorize') {
    handleAuthorization(url, response);
    return;
  }

  if (method === 'POST' && url.pathname === '/token') {
    await handleToken(request, response);
    return;
  }

  if (method === 'GET' && url.pathname === '/agents') {
    const agents = [...localSessions.values()].flatMap((session) => {
      const agent = session.getAgent();
      return agent === null
        ? []
        : [
            {
              ...agent,
              agentVersion: agent.hello.agentVersion,
              productId: agent.hello.productId,
              printerCount: session.getPrinters().length,
            },
          ];
    });
    sendJson(response, 200, { agents });
    return;
  }

  const printerRoute = /^\/agents\/([^/]+)\/printers$/.exec(url.pathname);
  if (printerRoute !== null) {
    requireMethod(method, 'GET');
    const agentId = decodePathSegment(printerRoute[1] ?? '');

    const session = localSessions.get(agentId);
    if (session?.getAgent() === null || session === undefined) {
      throw new HttpError(404, 'agent_offline', 'The requested agent is not connected.');
    }

    sendJson(response, 200, {
      agentId,
      printers: session.getPrinters(),
    });
    return;
  }

  const jobRoute = /^\/agents\/([^/]+)\/jobs$/.exec(url.pathname);
  if (jobRoute !== null) {
    requireMethod(method, 'POST');
    const agentId = decodePathSegment(jobRoute[1] ?? '');
    const input = await readJson(request, MAX_JOB_REQUEST_BYTES);
    const job = parsePrintJob(input);
    const session = localSessions.get(agentId);
    const delivery =
      session === undefined
        ? ({
            ok: false,
            agentId,
            reason: 'agent-offline',
            retryable: true,
          } satisfies DeliveryFailure)
        : await session.sendJob(job);

    logEvent(
      delivery.ok ? 'job.delivered' : 'job.delivery_deferred',
      delivery.ok
        ? {
            agentId,
            jobId: job.jobId,
            messageId: delivery.messageId,
          }
        : {
            agentId,
            jobId: job.jobId,
            reason: delivery.reason,
          },
    );

    sendJson(response, delivery.ok ? 202 : 503, {
      delivery,
      ...(delivery.ok
        ? {
            note: 'Delivery is not persistence or physical-print confirmation; wait for job lifecycle callbacks.',
          }
        : {
            note: 'The caller must retain or durably queue this job before retrying.',
          }),
    });
    return;
  }

  throw new HttpError(404, 'not_found', 'No route matched the request.');
}

function handleAuthorization(url: URL, response: ServerResponse): void {
  const request = authorization.parseAuthorizationRequest(url);

  if (url.searchParams.get('approve') === 'true') {
    const redirect = authorization.approveAuthorization(request);
    logEvent('authorization.approved', {
      clientId: request.clientId,
      redirectHost: new URL(request.redirectUri).host,
    });
    response.writeHead(302, {
      'cache-control': 'no-store',
      location: redirect.toString(),
      'referrer-policy': 'no-referrer',
    });
    response.end();
    return;
  }

  const approvalUrl = new URL(url);
  approvalUrl.searchParams.set('approve', 'true');
  sendHtml(response, 200, authorizationPage(approvalUrl.toString(), request.redirectUri));
}

async function handleToken(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const parameters = await readForm(request, MAX_FORM_BYTES);
  const token = authorization.exchangeAuthorizationCode(parameters);

  logEvent('authorization.token_issued', {
    agentId: token.agentId,
    expiresIn: token.expiresIn,
  });
  sendJson(response, 200, {
    access_token: token.accessToken,
    token_type: token.tokenType,
    expires_in: token.expiresIn,
    agent_id: token.agentId,
  });
}

function handleHttpError(response: ServerResponse, error: unknown): void {
  if (response.headersSent || response.destroyed) {
    response.destroy();
    return;
  }

  if (error instanceof HttpError) {
    sendJson(response, error.status, {
      error: error.code,
      error_description: error.message,
    });
    return;
  }

  if (error instanceof DevelopmentAuthError) {
    sendJson(response, 400, {
      error: error.error,
      error_description: error.message,
    });
    return;
  }

  if (error instanceof ProtocolError) {
    sendJson(response, 400, {
      error: 'invalid_print_job',
      error_description: error.message,
    });
    return;
  }

  logEvent('http.unhandled_error', {
    errorName: error instanceof Error ? error.name : 'UnknownError',
  });
  sendJson(response, 500, {
    error: 'internal_server_error',
    error_description: 'The example server could not process the request.',
  });
}

function authorizationPage(approvalUrl: string, redirectUri: string): string {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Authorize OPPA</title>
    <style>
      :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
      body { display: grid; min-height: 100vh; margin: 0; place-items: center; background: #111827; color: #f9fafb; }
      main { width: min(32rem, calc(100% - 3rem)); border: 1px solid #374151; border-radius: 1rem; padding: 2rem; background: #1f2937; box-shadow: 0 1rem 3rem #0006; }
      h1 { margin-top: 0; }
      code { overflow-wrap: anywhere; color: #bfdbfe; }
      .warning { border-left: .25rem solid #f59e0b; padding-left: 1rem; color: #fde68a; }
      a { display: inline-block; margin-top: 1rem; border-radius: .5rem; padding: .75rem 1rem; background: #2563eb; color: white; font-weight: 700; text-decoration: none; }
    </style>
  </head>
  <body>
    <main>
      <h1>Authorize OPPA</h1>
      <p class="warning">Development only. This local server is not a production identity provider.</p>
      <p>Allow the local OPPA agent to connect to this OpenPrinter example?</p>
      <p>Loopback callback: <code>${escapeHtml(redirectUri)}</code></p>
      <a href="${escapeHtml(approvalUrl)}">Authorize local agent</a>
    </main>
  </body>
</html>`;
}

function dashboardPage(host: string, port: number): string {
  const base = /* html */ `http://${host}:${port}`;
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>OpenPrinter — Development Server</title>
    <style>
      :root {
        color-scheme: light dark;
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        font-size: 14px;
        --bg: #f5f5f7;
        --card: #ffffff;
        --text: #1d1d1f;
        --muted: #6e6e73;
        --border: rgba(0,0,0,0.1);
        --primary: #e07428;
        --success: #34c759;
        --danger: #ff3b30;
        --radius: 12px;
      }
      @media (prefers-color-scheme: dark) {
        :root {
          --bg: #0a0a0a;
          --card: #1c1c1e;
          --text: #f5f5f7;
          --muted: #8e8e93;
          --border: rgba(255,255,255,0.1);
        }
      }
      *, *::before, *::after { box-sizing: border-box; margin: 0; }
      body { background: var(--bg); color: var(--text); min-height: 100vh; padding: 2rem 1.5rem; }
      .header { max-width: 760px; margin: 0 auto 2rem; }
      .header h1 { font-size: 1.5rem; font-weight: 700; }
      .header p { color: var(--muted); margin-top: 0.25rem; font-size: 0.875rem; }
      .badge { display: inline-block; background: #ff9f0a22; color: #b97a00; border-radius: 6px; font-size: 0.75rem; font-weight: 600; padding: 2px 8px; margin-top: 0.5rem; }
      @media (prefers-color-scheme: dark) { .badge { color: #ffcc00; background: #ffcc0022; } }
      .container { max-width: 760px; margin: 0 auto; }
      .card { background: var(--card); border-radius: var(--radius); border: 1px solid var(--border); overflow: hidden; margin-bottom: 1.25rem; }
      .card-header { padding: 1rem 1.25rem; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 0.75rem; }
      .card-header h2 { font-size: 0.9rem; font-weight: 600; }
      .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
      .dot-green { background: var(--success); }
      .dot-gray { background: var(--muted); }
      .card-body { padding: 1.25rem; }
      .empty { color: var(--muted); font-size: 0.875rem; text-align: center; padding: 2rem; }
      .agent-item { padding: 1rem; border: 1px solid var(--border); border-radius: 10px; margin-bottom: 0.75rem; }
      .agent-item:last-child { margin-bottom: 0; }
      .agent-id { font-family: monospace; font-size: 0.8rem; color: var(--muted); margin-top: 0.25rem; word-break: break-all; }
      .agent-meta { color: var(--muted); font-size: 0.8rem; margin-top: 0.5rem; }
      .printers { margin-top: 1rem; }
      .printer-row { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.75rem 1rem; background: var(--bg); border-radius: 8px; margin-top: 0.5rem; }
      .printer-info { flex: 1; min-width: 0; }
      .printer-name { font-weight: 500; font-size: 0.875rem; }
      .printer-sub { color: var(--muted); font-size: 0.75rem; margin-top: 0.1rem; }
      .btn { display: inline-flex; align-items: center; gap: 0.4rem; padding: 0.45rem 0.9rem; border-radius: 8px; font-size: 0.8rem; font-weight: 600; cursor: pointer; border: none; transition: opacity 0.15s; }
      .btn:hover { opacity: 0.85; }
      .btn:disabled { opacity: 0.45; cursor: not-allowed; }
      .btn-primary { background: var(--primary); color: white; }
      .btn-outline { background: transparent; border: 1px solid var(--border); color: var(--text); }
      .result { margin-top: 1rem; padding: 0.85rem 1rem; border-radius: 10px; font-size: 0.8rem; font-family: monospace; white-space: pre-wrap; word-break: break-all; border: 1px solid var(--border); background: var(--bg); display: none; }
      .result.show { display: block; }
      .result.ok { border-color: #34c75940; background: #34c75910; }
      .result.err { border-color: #ff3b3040; background: #ff3b3010; }
      .section-title { font-size: 0.75rem; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem; }
      .refresh-row { display: flex; justify-content: flex-end; margin-bottom: 0.75rem; }
    </style>
  </head>
  <body>
    <div class="header">
      <h1>OpenPrinter</h1>
      <p>Development server — ${escapeHtml(base)}</p>
      <span class="badge">⚠ Development only</span>
    </div>

    <div class="container">
      <div class="card" id="agents-card">
        <div class="card-header">
          <span class="dot dot-gray" id="status-dot"></span>
          <h2>Connected agents</h2>
          <button class="btn btn-outline" style="margin-left:auto;font-size:0.75rem;" onclick="loadAgents()">Refresh</button>
        </div>
        <div class="card-body" id="agents-body">
          <p class="empty">Loading…</p>
        </div>
      </div>

      <div id="result-global" class="result"></div>
    </div>

    <script>
      const BASE = ${JSON.stringify(base)};

      async function loadAgents() {
        const body = document.getElementById('agents-body');
        const dot = document.getElementById('status-dot');
        body.innerHTML = '<p class="empty">Loading…</p>';
        dot.className = 'dot dot-gray';
        try {
          const r = await fetch(BASE + '/agents');
          const data = await r.json();
          const agents = data.agents ?? [];
          dot.className = 'dot ' + (agents.length > 0 ? 'dot-green' : 'dot-gray');
          if (agents.length === 0) {
            body.innerHTML = '<p class="empty">No agents connected.<br>Open OPPA and complete authorization to connect.</p>';
            return;
          }
          body.innerHTML = '';
          for (const agent of agents) {
            const div = document.createElement('div');
            div.className = 'agent-item';
            div.innerHTML = \`
              <div style="display:flex;align-items:center;gap:0.5rem;">
                <span class="dot dot-green"></span>
                <strong style="font-size:0.9rem;">\${esc(agent.productId ?? 'Unknown agent')}</strong>
                <span style="color:var(--muted);font-size:0.75rem;">\${esc(agent.agentVersion ?? '')}</span>
              </div>
              <div class="agent-id">\${esc(agent.agentId)}</div>
              <div class="agent-meta">Connected \${formatRelative(agent.connectedAt)} · \${agent.printerCount ?? 0} printer(s)</div>
              <div class="printers" id="printers-\${esc(agent.agentId)}">
                <div class="section-title" style="margin-top:0.75rem;">Printers</div>
                <p class="empty" style="padding:0.5rem;">Loading printers…</p>
              </div>
            \`;
            body.appendChild(div);
            loadPrinters(agent.agentId);
          }
        } catch (e) {
          dot.className = 'dot dot-gray';
          body.innerHTML = '<p class="empty">Could not reach server.</p>';
        }
      }

      async function loadPrinters(agentId) {
        const container = document.getElementById('printers-' + agentId);
        if (!container) return;
        try {
          const r = await fetch(BASE + '/agents/' + encodeURIComponent(agentId) + '/printers');
          const data = await r.json();
          const printers = data.printers ?? [];
          if (printers.length === 0) {
            container.innerHTML = '<div class="section-title" style="margin-top:0.75rem;">Printers</div><p class="empty" style="padding:0.5rem;">No printers enabled on this agent.</p>';
            return;
          }
          container.innerHTML = '<div class="section-title" style="margin-top:0.75rem;">Printers</div>';
          for (const p of printers) {
            const row = document.createElement('div');
            row.className = 'printer-row';
            const resultId = 'result-' + agentId + '-' + p.id.replace(/[^a-z0-9]/gi, '-');
            const isOnline = p.availability === 'online';
            const isEnabled = p.enabled !== false;
            const canPrint = isOnline && isEnabled;
            const availabilityLabel =
              p.availability === 'online'
                ? 'Ready'
                : p.availability === 'offline'
                  ? 'Offline'
                  : 'Unknown';
            const connectionLabel =
              p.kind === 'local'
                ? 'Local printer'
                : p.kind === 'network'
                  ? 'Network printer'
                  : 'Virtual printer';
            row.innerHTML = \`
                <div class="printer-info">
                  <div class="printer-name">\${esc(p.name)}</div>
                  <div class="printer-sub">\${esc(connectionLabel)} · \${availabilityLabel}\${isEnabled ? '' : ' · Disabled'} · \${esc(p.id)}</div>
                </div>
                <button class="btn btn-primary" \${canPrint ? '' : 'disabled'} data-result-id="\${esc(resultId)}" data-agent-id="\${esc(agentId)}" data-printer-id="\${esc(p.id)}">
                  ↗ Test print
                </button>
              \`;
            const button = row.querySelector('button');
            button?.addEventListener('click', () => {
              void sendTestPrint(agentId, p.id, resultId);
            });
            container.appendChild(row);
            const resultDiv = document.createElement('div');
            resultDiv.id = resultId;
            resultDiv.className = 'result';
            container.appendChild(resultDiv);
          }
        } catch (e) {
          container.innerHTML = '<div class="section-title" style="margin-top:0.75rem;">Printers</div><p class="empty" style="padding:0.5rem;">Could not load printers.</p>';
        }
      }

      async function sendTestPrint(agentId, printerId, resultId) {
        const resultDiv = document.getElementById(resultId);
        if (!resultDiv) return;
        resultDiv.className = 'result show';
        resultDiv.textContent = 'Sending…';

        const job = {
          jobId: crypto.randomUUID(),
          idempotencyKey: crypto.randomUUID(),
          printerId: printerId,
          createdAt: new Date().toISOString(),
          document: {
            width: 80,
            sections: [
              { type: 'text', value: 'Test Print', bold: true, align: 'center' },
              { type: 'divider' },
              { type: 'text', value: 'OpenPrinter development server' },
              { type: 'text', value: new Date().toLocaleString() },
              { type: 'feed', lines: 3 },
              { type: 'cut' },
            ],
          },
        };

        try {
          const r = await fetch(BASE + '/agents/' + encodeURIComponent(agentId) + '/jobs', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(job),
          });
          const data = await r.json();
          resultDiv.className = 'result show ' + (r.ok ? 'ok' : 'err');
          resultDiv.textContent = JSON.stringify(data, null, 2);
        } catch (e) {
          resultDiv.className = 'result show err';
          resultDiv.textContent = 'Error: ' + (e instanceof Error ? e.message : String(e));
        }
      }

      function esc(str) {
        if (str == null) return '';
        return String(str).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
      }

      function formatRelative(iso) {
        if (!iso) return '';
        const diff = Math.round((Date.now() - new Date(iso).getTime()) / 1000);
        if (diff < 60) return diff + 's ago';
        if (diff < 3600) return Math.round(diff / 60) + 'm ago';
        return Math.round(diff / 3600) + 'h ago';
      }

      loadAgents();
      setInterval(loadAgents, 15000);
    </script>
  </body>
</html>`;
}

function requireMethod(actual: string, expected: string): void {
  if (actual !== expected) {
    throw new HttpError(405, 'method_not_allowed', `Use ${expected} for this endpoint.`);
  }
}

function decodePathSegment(value: string): string {
  try {
    const decoded = decodeURIComponent(value);
    if (decoded.length < 1 || decoded.length > 128 || decoded.includes('/')) {
      throw new Error('Invalid identifier length.');
    }
    return decoded;
  } catch {
    throw new HttpError(400, 'invalid_agent_id', 'The agent ID path segment is invalid.');
  }
}

function bearerToken(header: string | readonly string[] | undefined): string | null {
  if (typeof header !== 'string') {
    return null;
  }

  const match = /^Bearer ([A-Za-z0-9\-._~+/]+={0,})$/i.exec(header);
  return match?.[1] ?? null;
}

function rejectUpgrade(socket: Duplex, status: number, reason: string): void {
  if (socket.destroyed) {
    return;
  }

  socket.end(
    `HTTP/1.1 ${status} ${reason.replaceAll(/[\r\n]/g, ' ')}\r\n` +
      'Connection: close\r\n' +
      'Content-Length: 0\r\n' +
      'Cache-Control: no-store\r\n' +
      '\r\n',
  );
}

function sendWebSocketMessage(socket: WebSocket, message: string): Promise<void> {
  if (socket.readyState !== WebSocket.OPEN) {
    return Promise.reject(new Error('The WebSocket connection is not open.'));
  }

  return new Promise<void>((resolve, reject) => {
    socket.send(message, (error?: Error | null) => {
      if (error === undefined || error === null) {
        resolve();
      } else {
        reject(error);
      }
    });
  });
}

function closeWebSocket(socket: WebSocket, request: OpenPrinterTransportCloseRequest): void {
  if (socket.readyState === WebSocket.OPEN) {
    socket.close(webSocketCloseCode(request.reason), boundedWebSocketReason(request.detail ?? request.reason));
  } else if (socket.readyState !== WebSocket.CLOSED) {
    socket.terminate();
  }
}

function webSocketCloseCode(reason: OpenPrinterTransportCloseRequest['reason']): number {
  switch (reason) {
    case 'server-disconnect':
      return 1_000;
    case 'protocol-error':
      return 1_002;
    case 'transport-error':
      return 1_011;
    case 'heartbeat-timeout':
      return 4_002;
  }
}

function boundedWebSocketReason(value: string): string {
  let sanitized = '';
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    sanitized += codePoint <= 0x1f || codePoint === 0x7f ? ' ' : character;
  }
  sanitized = sanitized.trim();
  let result = '';
  for (const character of sanitized) {
    if (Buffer.byteLength(result + character, 'utf8') > 120) {
      break;
    }
    result += character;
  }
  return result;
}

function rawDataToUtf8(data: RawData): string {
  if (Array.isArray(data)) {
    return Buffer.concat(data).toString('utf8');
  }
  if (data instanceof ArrayBuffer) {
    return Buffer.from(data).toString('utf8');
  }
  return data.toString('utf8');
}

function parsePort(value: string | undefined): number {
  if (value === undefined) {
    return 8_787;
  }

  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error('PORT must be an integer between 1 and 65535.');
  }
  return port;
}

function logEvent(event: string, fields: Readonly<Record<string, unknown>>): void {
  console.info(
    JSON.stringify({
      timestamp: new Date().toISOString(),
      event,
      ...fields,
    }),
  );
}

async function shutdown(signal: 'SIGINT' | 'SIGTERM'): Promise<void> {
  if (shutdownStarted) {
    return;
  }
  shutdownStarted = true;
  logEvent('server.shutdown', { signal });

  authorization.revokeAll();
  await Promise.all(
    [...acceptedSessions].map((session) =>
      session.disconnect({
        code: 'server_shutdown',
        reason: 'The example server is shutting down.',
        reconnect: true,
      }),
    ),
  );
  await new Promise<void>((resolve) => {
    webSocketServer.close(() => resolve());
  });
  await new Promise<void>((resolve) => {
    httpServer.close(() => resolve());
    httpServer.closeAllConnections();
  });
}
