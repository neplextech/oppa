import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';

import { parsePrintJob, ProtocolError } from '@openprinter/protocol';
import { createOpenPrinterServer } from '@openprinter/server';

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

const openPrinter = createOpenPrinterServer({
  path: '/openprinter/agent',
  serverId: 'oppa-node-example',
  serverVersion: '0.1.0',
  authenticateAgent: ({ token }) => {
    const identity = authorization.authenticateAccessToken(token);

    if (identity === null) {
      return null;
    }

    return {
      agentId: identity.agentId,
      metadata: {
        issuedAt: identity.issuedAt,
        expiresAt: identity.expiresAt,
      },
    };
  },
  onAgentConnected: ({ agent }) => {
    logEvent('agent.connected', {
      agentId: agent.agentId,
      productId: agent.hello.productId,
      agentVersion: agent.hello.agentVersion,
    });
  },
  onAgentDisconnected: ({ agent, reason, closeCode }) => {
    logEvent('agent.disconnected', {
      agentId: agent.agentId,
      reason,
      ...(closeCode === undefined ? {} : { closeCode }),
    });
  },
  onAuthenticationFailed: ({ reason, remoteAddress }) => {
    logEvent('agent.authentication_failed', {
      reason,
      ...(remoteAddress === undefined ? {} : { remoteAddress }),
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

httpServer.on('upgrade', openPrinter.handleUpgrade);
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

async function handleRequest(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const method = request.method ?? 'GET';
  const url = new URL(request.url ?? '/', `http://${HOST}:${PORT}`);

  if (method === 'GET' && url.pathname === '/') {
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
    const agents = openPrinter.listAgents().map((agent) => ({
      ...agent,
      printerCount: openPrinter.getPrinters(agent.agentId).length,
    }));
    sendJson(response, 200, { agents });
    return;
  }

  const printerRoute = /^\/agents\/([^/]+)\/printers$/.exec(url.pathname);
  if (printerRoute !== null) {
    requireMethod(method, 'GET');
    const agentId = decodePathSegment(printerRoute[1] ?? '');

    if (openPrinter.getAgent(agentId) === null) {
      throw new HttpError(404, 'agent_offline', 'The requested agent is not connected.');
    }

    sendJson(response, 200, {
      agentId,
      printers: openPrinter.getPrinters(agentId),
    });
    return;
  }

  const jobRoute = /^\/agents\/([^/]+)\/jobs$/.exec(url.pathname);
  if (jobRoute !== null) {
    requireMethod(method, 'POST');
    const agentId = decodePathSegment(jobRoute[1] ?? '');
    const input = await readJson(request, MAX_JOB_REQUEST_BYTES);
    const job = parsePrintJob(input);
    const delivery = await openPrinter.sendJob(agentId, job);

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
  await openPrinter.close();
  await new Promise<void>((resolve) => {
    httpServer.close(() => resolve());
    httpServer.closeAllConnections();
  });
}
