import type { PrinterDescriptor } from '@openprinter/protocol';
import { WebSocket } from 'ws';

import { callbackErrorMessage } from './internal.js';
import type { AgentMessageOf, AgentDisconnectReason, AuthenticatedAgent, ConnectedAgent } from './types.js';

export type SessionState = 'handshaking' | 'connected' | 'closing' | 'closed';

export interface AgentSession<Metadata> {
  readonly socket: WebSocket;
  readonly identity: AuthenticatedAgent<Metadata>;
  readonly sessionId: string;
  readonly printers: Map<string, PrinterDescriptor>;
  readonly pendingHeartbeats: Map<string, number>;
  state: SessionState;
  hello: AgentMessageOf<'agent.hello'>['payload'] | null;
  connectedAtMs: number | null;
  lastSeenAtMs: number;
  lastHeartbeatAtMs: number;
  printerRevision: number | null;
  handshakeTimer: ReturnType<typeof setTimeout> | null;
  heartbeatDeadline: ReturnType<typeof setTimeout> | null;
  processing: Promise<void>;
  disconnectReason: AgentDisconnectReason | null;
  transportError: string | null;
  payloadErrorReported: boolean;
}

export const WS_CLOSE_CONNECTION_REPLACED = 4_001;
export const WS_CLOSE_HEARTBEAT_TIMEOUT = 4_002;
export const WS_CLOSE_SERVER_SHUTDOWN = 4_003;
export const WS_CLOSE_HANDSHAKE_TIMEOUT = 4_004;

export function createAgentSession<Metadata>(
  socket: WebSocket,
  identity: AuthenticatedAgent<Metadata>,
  sessionId: string,
): AgentSession<Metadata> {
  const now = Date.now();
  return {
    socket,
    identity,
    sessionId,
    printers: new Map(),
    pendingHeartbeats: new Map(),
    state: 'handshaking',
    hello: null,
    connectedAtMs: null,
    lastSeenAtMs: now,
    lastHeartbeatAtMs: now,
    printerRevision: null,
    handshakeTimer: null,
    heartbeatDeadline: null,
    processing: Promise.resolve(),
    disconnectReason: null,
    transportError: null,
    payloadErrorReported: false,
  };
}

export function snapshotSession<Metadata>(session: AgentSession<Metadata>): ConnectedAgent<Metadata> {
  if (session.hello === null || session.connectedAtMs === null) {
    throw new Error('Cannot expose a session before the protocol handshake.');
  }

  return {
    agentId: session.identity.agentId,
    sessionId: session.sessionId,
    ...(session.identity.metadata === undefined ? {} : { metadata: session.identity.metadata }),
    hello: structuredClone(session.hello),
    connectedAt: new Date(session.connectedAtMs).toISOString(),
    lastSeenAt: new Date(session.lastSeenAtMs).toISOString(),
    printerRevision: session.printerRevision,
  };
}

export async function sendEncoded<Metadata>(session: AgentSession<Metadata>, encoded: string): Promise<boolean> {
  if (session.state === 'closed' || session.socket.readyState !== WebSocket.OPEN) {
    return false;
  }

  return await new Promise<boolean>((resolve) => {
    session.socket.send(encoded, (error?: Error | null) => {
      // `ws` documents `undefined` on success but currently invokes the
      // callback with `null`; accept both without treating a successful
      // write as a transport failure.
      if (error === undefined || error === null) {
        resolve(true);
        return;
      }

      session.transportError = callbackErrorMessage(error);
      resolve(false);
      session.socket.terminate();
    });
  });
}

export function closeSocket(socket: WebSocket, code: number, reason: string): void {
  if (socket.readyState === WebSocket.OPEN) {
    socket.close(code, reason);
    return;
  }

  if (socket.readyState !== WebSocket.CLOSED) {
    socket.terminate();
  }
}
