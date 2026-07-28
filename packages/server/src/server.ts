import { randomUUID } from 'node:crypto';
import type { IncomingMessage } from 'node:http';
import type { Duplex } from 'node:stream';
import { isDeepStrictEqual } from 'node:util';

import {
  decodeAgentMessage,
  encodeServerMessage,
  MAX_WIRE_MESSAGE_BYTES,
  PROTOCOL_VERSION,
  ProtocolError,
  type AgentMessage,
  type PrintJob,
  type PrinterDescriptor,
  type ServerMessage,
} from '@openprinter/protocol';
import { WebSocket, WebSocketServer, type RawData } from 'ws';

import {
  boundedCloseDetail,
  callbackErrorMessage,
  isValidIdentifier,
  parseBearerToken,
  rawDataToBytes,
  rejectUpgrade,
  validateOptions,
} from './internal.js';
import {
  closeSocket,
  createAgentSession,
  sendEncoded,
  snapshotSession,
  type AgentSession,
  WS_CLOSE_CONNECTION_REPLACED,
  WS_CLOSE_HANDSHAKE_TIMEOUT,
  WS_CLOSE_HEARTBEAT_TIMEOUT,
  WS_CLOSE_SERVER_SHUTDOWN,
} from './session.js';
import type {
  AgentDisconnectReason,
  AgentMessageOf,
  AuthenticatedAgent,
  AuthenticationFailedEvent,
  CallbackErrorEvent,
  ConfigurationInvalidation,
  ConnectedAgent,
  DeliveryFailure,
  DeliveryResult,
  DisconnectAgentOptions,
  JobCancellation,
  OpenPrinterCallbackName,
  OpenPrinterServer,
  OpenPrinterServerOptions,
  PrintersChangedEvent,
  ServerMessageOf,
  ServerProtocolErrorCode,
} from './types.js';

interface ResolvedOptions {
  readonly authenticationTimeoutMs: number;
  readonly callbackTimeoutMs: number;
  readonly handshakeTimeoutMs: number;
  readonly heartbeatIntervalMs: number;
  readonly heartbeatTimeoutMs: number;
  readonly maxMessageBytes: number;
  readonly path?: string;
  readonly serverId: string;
  readonly serverVersion: string;
}

class AuthenticationTimeoutError extends Error {
  public constructor() {
    super('The authentication callback exceeded its configured timeout.');
    this.name = 'AuthenticationTimeoutError';
  }
}

class HostCallbackTimeoutError extends Error {
  public constructor(callback: OpenPrinterCallbackName) {
    super(`The ${callback} host callback exceeded the configured lifecycle callback timeout.`);
    this.name = 'HostCallbackTimeoutError';
  }
}

/**
 * Create a framework-neutral OpenPrinter server for an existing HTTP server.
 *
 * The returned object owns only live WebSocket sessions. It never creates a
 * durable queue or persists jobs.
 */
export function createOpenPrinterServer<Metadata>(
  options: OpenPrinterServerOptions<Metadata>,
): OpenPrinterServer<Metadata> {
  return new OpenPrinterServerImplementation(options);
}

class OpenPrinterServerImplementation<Metadata> implements OpenPrinterServer<Metadata> {
  readonly #options: OpenPrinterServerOptions<Metadata>;
  readonly #resolved: ResolvedOptions;
  readonly #webSocketServer: WebSocketServer;
  readonly #sessions = new Map<string, AgentSession<Metadata>>();
  readonly #allSessions = new Set<AgentSession<Metadata>>();
  readonly #pendingUpgradeSockets = new Set<Duplex>();
  readonly #heartbeatTimer: ReturnType<typeof setInterval>;
  #closed = false;

  public constructor(options: OpenPrinterServerOptions<Metadata>) {
    this.#options = options;
    this.#resolved = validateOptions(options, MAX_WIRE_MESSAGE_BYTES);
    this.#webSocketServer = new WebSocketServer({
      clientTracking: false,
      maxPayload: this.#resolved.maxMessageBytes,
      noServer: true,
      perMessageDeflate: false,
    });
    this.#heartbeatTimer = setInterval(() => {
      this.#sendHeartbeatRequests();
    }, this.#resolved.heartbeatIntervalMs);
    this.#heartbeatTimer.unref();
  }

  public get closed(): boolean {
    return this.#closed;
  }

  public readonly handleUpgrade = (request: IncomingMessage, socket: Duplex, head: Buffer): void => {
    void this.#upgrade(request, socket, head).catch(() => {
      rejectUpgrade(socket, 500, 'Internal Server Error');
    });
  };

  public listAgents(): readonly ConnectedAgent<Metadata>[] {
    return [...this.#sessions.values()].map((session) => snapshotSession(session));
  }

  public getAgent(agentId: string): ConnectedAgent<Metadata> | null {
    const session = this.#sessions.get(agentId);
    return session === undefined ? null : snapshotSession(session);
  }

  public getPrinters(agentId: string): readonly PrinterDescriptor[] {
    const session = this.#sessions.get(agentId);
    return session === undefined ? [] : structuredClone([...session.printers.values()]);
  }

  public async send(agentId: string, message: ServerMessage): Promise<DeliveryResult> {
    // Encode before consulting connection state so programmer errors never
    // masquerade as an ordinary offline result.
    const encoded = encodeServerMessage(message);
    const encodedBytes = Buffer.byteLength(encoded, 'utf8');
    if (encodedBytes > this.#resolved.maxMessageBytes) {
      throw new ProtocolError(
        'message_too_large',
        `The encoded server message is ${encodedBytes} bytes; this server's configured limit is ${this.#resolved.maxMessageBytes} bytes.`,
      );
    }

    if (this.#closed) {
      return deliveryFailure(agentId, 'server-closed');
    }

    const session = this.#sessions.get(agentId);
    if (session === undefined) {
      return deliveryFailure(agentId, 'agent-offline');
    }

    if (!(await sendEncoded(session, encoded))) {
      return deliveryFailure(agentId, 'connection-closed');
    }

    return {
      ok: true,
      agentId,
      messageId: message.messageId,
      sentAt: message.sentAt,
    };
  }

  public sendJob(agentId: string, job: PrintJob): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.print_job'> = {
      ...newEnvelope(),
      type: 'server.print_job',
      payload: job,
    };
    return this.send(agentId, message);
  }

  public requestPrinters(agentId: string): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.request_printer_inventory'> = {
      ...newEnvelope(),
      type: 'server.request_printer_inventory',
      payload: {},
    };
    return this.send(agentId, message);
  }

  public cancelJob(agentId: string, cancellation: JobCancellation): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.cancel_job'> = {
      ...newEnvelope(),
      type: 'server.cancel_job',
      payload: cancellation,
    };
    return this.send(agentId, message);
  }

  public invalidateConfiguration(agentId: string, invalidation: ConfigurationInvalidation): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.configuration_invalidated'> = {
      ...newEnvelope(),
      type: 'server.configuration_invalidated',
      payload: invalidation,
    };
    return this.send(agentId, message);
  }

  public async disconnect(agentId: string, options: DisconnectAgentOptions = {}): Promise<boolean> {
    const session = this.#sessions.get(agentId);
    if (session === undefined) {
      return false;
    }

    session.state = 'closing';
    session.disconnectReason = 'server-disconnect';
    const message = createDisconnectMessage({
      code: options.code ?? 'server_disconnect',
      reason: options.reason ?? 'The host application closed this connection.',
      reconnect: options.reconnect ?? true,
      ...(options.retryAfterMs === undefined ? {} : { retryAfterMs: options.retryAfterMs }),
    });
    await sendEncoded(session, encodeServerMessage(message));
    closeSocket(session.socket, 1_000, 'Server disconnect');
    await this.#finalizeSession(session, 'server-disconnect');
    return true;
  }

  public async close(): Promise<void> {
    if (this.#closed) {
      return;
    }

    this.#closed = true;
    clearInterval(this.#heartbeatTimer);

    for (const socket of this.#pendingUpgradeSockets) {
      rejectUpgrade(socket, 503, 'Service Unavailable');
    }
    this.#pendingUpgradeSockets.clear();

    await Promise.all(
      [...this.#allSessions].map(async (session) => {
        if (session.state === 'closed') {
          return;
        }

        const wasConnected = session.state === 'connected';
        session.state = 'closing';
        session.disconnectReason = 'server-shutdown';

        if (wasConnected && session.socket.readyState === WebSocket.OPEN) {
          const message = createDisconnectMessage({
            code: 'server_shutdown',
            reason: 'The OpenPrinter server is shutting down.',
            reconnect: true,
          });
          await sendEncoded(session, encodeServerMessage(message));
        }

        closeSocket(session.socket, WS_CLOSE_SERVER_SHUTDOWN, 'Server shutdown');
        await this.#finalizeSession(session, 'server-shutdown');
      }),
    );
  }

  async #upgrade(request: IncomingMessage, socket: Duplex, head: Buffer): Promise<void> {
    if (this.#closed) {
      rejectUpgrade(socket, 503, 'Service Unavailable');
      return;
    }

    let pathname: string;
    try {
      pathname = new URL(request.url ?? '/', 'http://openprinter.invalid').pathname;
    } catch {
      rejectUpgrade(socket, 400, 'Bad Request');
      return;
    }

    if (this.#resolved.path !== undefined && pathname !== this.#resolved.path) {
      rejectUpgrade(socket, 404, 'Not Found');
      return;
    }

    const bearer = parseBearerToken(request.headers.authorization);
    if (!bearer.ok) {
      const event: AuthenticationFailedEvent = {
        reason: bearer.reason,
        ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
      };
      rejectUpgrade(socket, 401, 'Unauthorized');
      await this.#authenticationFailed(event);
      return;
    }

    this.#pendingUpgradeSockets.add(socket);
    const controller = new AbortController();
    const abortAuthentication = (): void => {
      controller.abort();
    };
    socket.once('close', abortAuthentication);

    let timer: ReturnType<typeof setTimeout> | undefined;
    let identity: AuthenticatedAgent<Metadata> | null;

    try {
      const timeout = new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          controller.abort();
          reject(new AuthenticationTimeoutError());
        }, this.#resolved.authenticationTimeoutMs);
        timer.unref();
      });

      identity = await Promise.race([
        Promise.resolve(
          this.#options.authenticateAgent({
            token: bearer.token,
            request,
            signal: controller.signal,
          }),
        ),
        timeout,
      ]);
    } catch (error) {
      if (!socket.destroyed) {
        const event: AuthenticationFailedEvent = {
          reason: error instanceof AuthenticationTimeoutError ? 'timeout' : 'callback-error',
          ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
        };
        rejectUpgrade(
          socket,
          error instanceof AuthenticationTimeoutError ? 504 : 401,
          error instanceof AuthenticationTimeoutError ? 'Gateway Timeout' : 'Unauthorized',
        );
        await this.#authenticationFailed(event);
      }
      return;
    } finally {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
      socket.off('close', abortAuthentication);
      this.#pendingUpgradeSockets.delete(socket);
    }

    if (socket.destroyed) {
      return;
    }

    if (this.#closed) {
      rejectUpgrade(socket, 503, 'Service Unavailable');
      return;
    }

    if (identity === null) {
      const event: AuthenticationFailedEvent = {
        reason: 'rejected',
        ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
      };
      rejectUpgrade(socket, 401, 'Unauthorized');
      await this.#authenticationFailed(event);
      return;
    }

    if (typeof identity.agentId !== 'string' || !isValidIdentifier(identity.agentId)) {
      const event: AuthenticationFailedEvent = {
        reason: 'invalid-agent',
        ...(request.socket.remoteAddress === undefined ? {} : { remoteAddress: request.socket.remoteAddress }),
      };
      rejectUpgrade(socket, 403, 'Forbidden');
      await this.#authenticationFailed(event);
      return;
    }

    try {
      this.#webSocketServer.handleUpgrade(request, socket, head, (webSocket) => {
        this.#acceptConnection(webSocket, identity);
      });
    } catch {
      rejectUpgrade(socket, 400, 'Bad Request');
    }
  }

  #acceptConnection(socket: WebSocket, identity: AuthenticatedAgent<Metadata>): void {
    const session = createAgentSession(socket, identity, randomUUID());
    this.#allSessions.add(session);

    session.handshakeTimer = setTimeout(() => {
      void this.#protocolViolation(
        session,
        'handshake-timeout',
        new Error('The agent did not send agent.hello before the timeout.'),
        WS_CLOSE_HANDSHAKE_TIMEOUT,
      );
    }, this.#resolved.handshakeTimeoutMs);
    session.handshakeTimer.unref();

    socket.on('message', (data: RawData, isBinary: boolean) => {
      session.processing = session.processing
        .then(() => this.#receiveMessage(session, data, isBinary))
        .catch((error: unknown) =>
          this.#protocolViolation(
            session,
            'invalid-message',
            error instanceof Error ? error : new Error('Unexpected message processing failure.'),
            1_002,
          ),
        );
    });
    socket.on('close', (code: number, reason: Buffer) => {
      void this.#socketClosed(session, code, reason);
    });
    socket.on('error', (error: Error & { code?: string }) => {
      session.transportError = callbackErrorMessage(error);

      if (error.code === 'WS_ERR_UNSUPPORTED_MESSAGE_LENGTH' && !session.payloadErrorReported) {
        session.payloadErrorReported = true;
        void this.#protocolViolation(
          session,
          'message-too-large',
          new Error(`The WebSocket payload exceeded ${this.#resolved.maxMessageBytes} bytes.`),
          1_009,
        );
      }
    });
  }

  async #receiveMessage(session: AgentSession<Metadata>, data: RawData, isBinary: boolean): Promise<void> {
    if (session.state === 'closing' || session.state === 'closed') {
      return;
    }

    if (isBinary) {
      await this.#protocolViolation(
        session,
        'binary-message',
        new Error('OpenPrinter messages must use WebSocket text frames.'),
        1_003,
      );
      return;
    }

    const bytes = rawDataToBytes(data);
    if (bytes.byteLength > this.#resolved.maxMessageBytes) {
      await this.#protocolViolation(
        session,
        'message-too-large',
        new Error(`The WebSocket payload exceeded ${this.#resolved.maxMessageBytes} bytes.`),
        1_009,
      );
      return;
    }

    let message: AgentMessage;
    try {
      message = decodeAgentMessage(bytes);
    } catch (error) {
      const protocolError = error instanceof Error ? error : new Error('The agent message was invalid.');
      const code = protocolErrorCode(error);
      await this.#protocolViolation(session, code, protocolError, code === 'message-too-large' ? 1_009 : 1_002);
      return;
    }

    session.lastSeenAtMs = Date.now();

    if (session.state === 'handshaking') {
      if (message.type !== 'agent.hello') {
        await this.#protocolViolation(
          session,
          'unexpected-message',
          new Error('The first agent message must be agent.hello.'),
          1_002,
        );
        return;
      }

      await this.#completeHandshake(session, message);
      return;
    }

    if (message.type === 'agent.hello') {
      await this.#protocolViolation(
        session,
        'unexpected-message',
        new Error('agent.hello may only be sent once per connection.'),
        1_002,
      );
      return;
    }

    await this.#routeMessage(session, message);
  }

  async #completeHandshake(session: AgentSession<Metadata>, hello: AgentMessageOf<'agent.hello'>): Promise<void> {
    if (hello.payload.agentId !== session.identity.agentId) {
      await this.#protocolViolation(
        session,
        'identity-mismatch',
        new Error('The authenticated identity does not match the agent hello.'),
        1_008,
      );
      return;
    }

    if (!hello.payload.supportedProtocolVersions.includes(PROTOCOL_VERSION)) {
      await this.#protocolViolation(
        session,
        'unsupported-protocol-version',
        new Error('The peers do not share a supported protocol version.'),
        1_002,
      );
      return;
    }

    const response: ServerMessageOf<'server.hello'> = {
      ...newEnvelope(),
      correlationId: hello.messageId,
      type: 'server.hello',
      payload: {
        serverId: this.#resolved.serverId,
        serverVersion: this.#resolved.serverVersion,
        sessionId: session.sessionId,
        supportedProtocolVersions: [PROTOCOL_VERSION],
        selectedProtocolVersion: PROTOCOL_VERSION,
        heartbeatIntervalMs: this.#resolved.heartbeatIntervalMs,
        maxMessageBytes: this.#resolved.maxMessageBytes,
      },
    };

    if (!(await sendEncoded(session, encodeServerMessage(response)))) {
      closeSocket(session.socket, 1_001, 'Handshake delivery failed');
      return;
    }

    if (session.handshakeTimer !== null) {
      clearTimeout(session.handshakeTimer);
      session.handshakeTimer = null;
    }

    const now = Date.now();
    const previous = this.#sessions.get(session.identity.agentId);
    session.hello = hello.payload;
    session.connectedAtMs = now;
    session.lastHeartbeatAtMs = now;
    session.state = 'connected';
    this.#sessions.set(session.identity.agentId, session);

    if (previous !== undefined && previous !== session) {
      await this.#replaceSession(previous);
    }

    this.#armHeartbeatDeadline(session);
    await this.#invoke('onAgentConnected', this.#options.onAgentConnected, {
      agent: snapshotSession(session),
    });
    await this.#sendHeartbeat(session);
  }

  async #routeMessage(
    session: AgentSession<Metadata>,
    message: Exclude<AgentMessage, AgentMessageOf<'agent.hello'>>,
  ): Promise<void> {
    switch (message.type) {
      case 'agent.authentication_metadata':
        await this.#invoke('onAuthenticationMetadata', this.#options.onAuthenticationMetadata, {
          agent: snapshotSession(session),
          message,
        });
        return;

      case 'agent.heartbeat':
        if (!session.pendingHeartbeats.has(message.correlationId)) {
          await this.#protocolViolation(
            session,
            'unexpected-message',
            new Error('The heartbeat does not correlate to an outstanding request.'),
            1_002,
          );
          return;
        }

        session.pendingHeartbeats.delete(message.correlationId);
        session.lastHeartbeatAtMs = Date.now();
        this.#prunePendingHeartbeats(session);
        this.#armHeartbeatDeadline(session);
        return;

      case 'agent.printer_inventory':
        if (session.printerRevision !== null && message.payload.revision < session.printerRevision) {
          await this.#protocolViolation(
            session,
            'unexpected-message',
            new Error('The printer inventory revision regressed.'),
            1_002,
          );
          return;
        }
        if (
          session.printerRevision === message.payload.revision &&
          samePrinterInventory(session.printers, message.payload.printers)
        ) {
          return;
        }
        if (session.printerRevision === message.payload.revision) {
          await this.#protocolViolation(
            session,
            'unexpected-message',
            new Error('The printer inventory changed without advancing its revision.'),
            1_002,
          );
          return;
        }
        session.printers.clear();
        for (const printer of message.payload.printers) {
          session.printers.set(printer.id, printer);
        }
        session.printerRevision = message.payload.revision;
        await this.#printersChanged(session, 'snapshot', message);
        return;

      case 'agent.printer_inventory_changed':
        {
          const inventoryError = inventoryChangeError(session, message);
          if (inventoryError !== null) {
            await this.#protocolViolation(session, 'unexpected-message', new Error(inventoryError), 1_002);
            return;
          }
        }
        for (const printerId of message.payload.removedPrinterIds) {
          session.printers.delete(printerId);
        }
        for (const printer of message.payload.added) {
          session.printers.set(printer.id, printer);
        }
        for (const printer of message.payload.updated) {
          session.printers.set(printer.id, printer);
        }
        session.printerRevision = message.payload.revision;
        await this.#printersChanged(session, 'change', message);
        return;

      case 'agent.job_received':
        await this.#invoke('onJobReceived', this.#options.onJobReceived, {
          agent: snapshotSession(session),
          message,
        });
        return;

      case 'agent.job_submitted':
        await this.#invoke('onJobSubmitted', this.#options.onJobSubmitted, {
          agent: snapshotSession(session),
          message,
        });
        return;

      case 'agent.job_failed':
        await this.#invoke('onJobFailed', this.#options.onJobFailed, {
          agent: snapshotSession(session),
          message,
        });
        return;

      case 'agent.diagnostics':
        if (message.payload.agentId !== session.identity.agentId) {
          await this.#protocolViolation(
            session,
            'identity-mismatch',
            new Error('The diagnostics identity does not match the authenticated agent.'),
            1_008,
          );
          return;
        }

        await this.#invoke('onDiagnostics', this.#options.onDiagnostics, {
          agent: snapshotSession(session),
          message,
        });
        return;
    }
  }

  async #printersChanged(
    session: AgentSession<Metadata>,
    kind: PrintersChangedEvent<Metadata>['kind'],
    message: AgentMessageOf<'agent.printer_inventory'> | AgentMessageOf<'agent.printer_inventory_changed'>,
  ): Promise<void> {
    await this.#invoke('onPrintersChanged', this.#options.onPrintersChanged, {
      agent: snapshotSession(session),
      kind,
      revision: message.payload.revision,
      printers: this.getPrinters(session.identity.agentId),
      message,
    });
  }

  #sendHeartbeatRequests(): void {
    if (this.#closed) {
      return;
    }

    for (const session of this.#sessions.values()) {
      void this.#sendHeartbeat(session);
    }
  }

  async #sendHeartbeat(session: AgentSession<Metadata>): Promise<void> {
    if (session.state !== 'connected') {
      return;
    }

    this.#prunePendingHeartbeats(session);
    const message: ServerMessageOf<'server.heartbeat'> = {
      ...newEnvelope(),
      type: 'server.heartbeat',
      payload: {
        timeoutMs: this.#resolved.heartbeatTimeoutMs,
      },
    };
    const encoded = encodeServerMessage(message);
    session.pendingHeartbeats.set(message.messageId, Date.now());

    if (!(await sendEncoded(session, encoded))) {
      session.pendingHeartbeats.delete(message.messageId);
    }
  }

  #armHeartbeatDeadline(session: AgentSession<Metadata>): void {
    if (session.heartbeatDeadline !== null) {
      clearTimeout(session.heartbeatDeadline);
    }

    session.heartbeatDeadline = setTimeout(() => {
      void this.#heartbeatExpired(session);
    }, this.#resolved.heartbeatTimeoutMs);
    session.heartbeatDeadline.unref();
  }

  async #heartbeatExpired(session: AgentSession<Metadata>): Promise<void> {
    if (session.state !== 'connected') {
      return;
    }

    session.state = 'closing';
    session.disconnectReason = 'heartbeat-timeout';
    const timeoutEvent = {
      agent: snapshotSession(session),
      lastHeartbeatAt: new Date(session.lastHeartbeatAtMs).toISOString(),
      timeoutMs: this.#resolved.heartbeatTimeoutMs,
    };

    const message = createDisconnectMessage({
      code: 'heartbeat_timeout',
      reason: 'The agent did not answer protocol heartbeats in time.',
      reconnect: true,
      retryAfterMs: this.#resolved.heartbeatIntervalMs,
    });
    await sendEncoded(session, encodeServerMessage(message));
    closeSocket(session.socket, WS_CLOSE_HEARTBEAT_TIMEOUT, 'Heartbeat timeout');
    await this.#finalizeSession(session, 'heartbeat-timeout');
    await this.#invoke('onHeartbeatTimeout', this.#options.onHeartbeatTimeout, timeoutEvent);
  }

  #prunePendingHeartbeats(session: AgentSession<Metadata>): void {
    const cutoff = Date.now() - this.#resolved.heartbeatTimeoutMs;
    for (const [messageId, sentAt] of session.pendingHeartbeats) {
      if (sentAt < cutoff) {
        session.pendingHeartbeats.delete(messageId);
      }
    }
  }

  async #replaceSession(previous: AgentSession<Metadata>): Promise<void> {
    if (previous.state === 'closed') {
      return;
    }

    previous.state = 'closing';
    previous.disconnectReason = 'connection-replaced';
    const message = createDisconnectMessage({
      code: 'connection_replaced',
      reason: 'A newer connection replaced this agent session.',
      reconnect: false,
    });
    await sendEncoded(previous, encodeServerMessage(message));
    closeSocket(previous.socket, WS_CLOSE_CONNECTION_REPLACED, 'Connection replaced');
    await this.#finalizeSession(previous, 'connection-replaced');
  }

  async #protocolViolation(
    session: AgentSession<Metadata>,
    code: ServerProtocolErrorCode,
    error: Error,
    closeCode: number,
  ): Promise<void> {
    if (session.state === 'closed' || session.state === 'closing') {
      return;
    }

    const wasConnected = session.state === 'connected';
    const agent = wasConnected ? snapshotSession(session) : undefined;
    session.state = 'closing';
    session.disconnectReason = 'protocol-error';

    const protocolErrorEvent = {
      agentId: session.identity.agentId,
      ...(agent === undefined ? {} : { agent }),
      code,
      error,
    };

    if (wasConnected && session.socket.readyState === WebSocket.OPEN) {
      const message = createDisconnectMessage({
        code: 'protocol_error',
        reason: 'The connection violated the OpenPrinter protocol.',
        reconnect: false,
      });
      await sendEncoded(session, encodeServerMessage(message));
    }

    closeSocket(session.socket, closeCode, 'Protocol error');
    await this.#finalizeSession(session, 'protocol-error');
    await this.#invoke('onProtocolError', this.#options.onProtocolError, protocolErrorEvent);
  }

  async #socketClosed(session: AgentSession<Metadata>, code: number, reason: Buffer): Promise<void> {
    if (session.state === 'closed') {
      return;
    }

    const explicitReason = session.disconnectReason;
    const disconnectReason = explicitReason ?? (session.transportError === null ? 'peer-closed' : 'transport-error');
    const peerDetail = boundedCloseDetail(reason.toString('utf8'));
    const detail =
      peerDetail ?? (session.transportError === null ? undefined : boundedCloseDetail(session.transportError));
    await this.#finalizeSession(session, disconnectReason, {
      closeCode: code,
      ...(detail === undefined ? {} : { detail }),
    });
  }

  async #finalizeSession(
    session: AgentSession<Metadata>,
    reason: AgentDisconnectReason,
    close?: {
      readonly closeCode?: number;
      readonly detail?: string;
    },
  ): Promise<void> {
    if (session.state === 'closed') {
      return;
    }

    const wasConnected = session.connectedAtMs !== null && session.hello !== null;
    const snapshot = wasConnected ? snapshotSession(session) : null;
    session.state = 'closed';
    session.disconnectReason = reason;

    if (session.handshakeTimer !== null) {
      clearTimeout(session.handshakeTimer);
      session.handshakeTimer = null;
    }
    if (session.heartbeatDeadline !== null) {
      clearTimeout(session.heartbeatDeadline);
      session.heartbeatDeadline = null;
    }
    session.pendingHeartbeats.clear();
    this.#allSessions.delete(session);

    if (this.#sessions.get(session.identity.agentId) === session) {
      this.#sessions.delete(session.identity.agentId);
    }

    if (snapshot !== null) {
      await this.#invoke('onAgentDisconnected', this.#options.onAgentDisconnected, {
        agent: snapshot,
        reason,
        ...(close?.closeCode === undefined ? {} : { closeCode: close.closeCode }),
        ...(close?.detail === undefined ? {} : { detail: close.detail }),
      });
    }
  }

  async #authenticationFailed(event: AuthenticationFailedEvent): Promise<void> {
    await this.#invoke('onAuthenticationFailed', this.#options.onAuthenticationFailed, event);
  }

  async #invoke<Event>(
    name: Exclude<OpenPrinterCallbackName, 'onCallbackError'>,
    callback: ((event: Event) => void | PromiseLike<void>) | undefined,
    event: Event,
  ): Promise<void> {
    if (callback === undefined) {
      return;
    }

    try {
      await this.#boundedCallback(name, callback, event);
    } catch (error) {
      const callbackError: CallbackErrorEvent = {
        callback: name,
        error,
      };

      try {
        if (this.#options.onCallbackError !== undefined) {
          await this.#boundedCallback('onCallbackError', this.#options.onCallbackError, callbackError);
        }
      } catch {
        // Callback isolation terminates here to avoid recursive error events.
      }
    }
  }

  async #boundedCallback<Event>(
    name: OpenPrinterCallbackName,
    callback: (event: Event) => void | PromiseLike<void>,
    event: Event,
  ): Promise<void> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_resolve, reject) => {
      timer = setTimeout(() => {
        reject(new HostCallbackTimeoutError(name));
      }, this.#resolved.callbackTimeoutMs);
      timer.unref();
    });

    try {
      await Promise.race([Promise.resolve().then(() => callback(event)), timeout]);
    } finally {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
    }
  }
}

function samePrinterInventory(
  current: ReadonlyMap<string, PrinterDescriptor>,
  incoming: readonly PrinterDescriptor[],
): boolean {
  if (current.size !== incoming.length) {
    return false;
  }

  return incoming.every((printer) => {
    const existing = current.get(printer.id);
    return existing !== undefined && isDeepStrictEqual(existing, printer);
  });
}

function inventoryChangeError<Metadata>(
  session: AgentSession<Metadata>,
  message: AgentMessageOf<'agent.printer_inventory_changed'>,
): string | null {
  if (session.printerRevision === null) {
    return 'An incremental printer inventory requires an initial snapshot.';
  }
  if (message.payload.revision <= session.printerRevision) {
    return 'The incremental printer inventory revision did not advance.';
  }

  const added = new Set(message.payload.added.map((printer) => printer.id));
  const updated = new Set(message.payload.updated.map((printer) => printer.id));
  const removed = new Set(message.payload.removedPrinterIds);
  if (added.size !== message.payload.added.length || updated.size !== message.payload.updated.length) {
    return 'The incremental printer inventory repeats a printer ID.';
  }
  if ([...added].some((id) => updated.has(id) || removed.has(id)) || [...updated].some((id) => removed.has(id))) {
    return 'The incremental printer inventory contains overlapping operations.';
  }
  if ([...added].some((id) => session.printers.has(id))) {
    return 'The incremental printer inventory adds an existing printer.';
  }
  if ([...updated, ...removed].some((id) => !session.printers.has(id))) {
    return 'The incremental printer inventory changes an unknown printer.';
  }
  return null;
}

function newEnvelope(): {
  readonly protocolVersion: typeof PROTOCOL_VERSION;
  readonly messageId: string;
  readonly sentAt: string;
} {
  return {
    protocolVersion: PROTOCOL_VERSION,
    messageId: randomUUID(),
    sentAt: new Date().toISOString(),
  };
}

function createDisconnectMessage(
  payload: ServerMessageOf<'server.disconnect'>['payload'],
): ServerMessageOf<'server.disconnect'> {
  return {
    ...newEnvelope(),
    type: 'server.disconnect',
    payload,
  };
}

function deliveryFailure(agentId: string, reason: DeliveryFailure['reason']): DeliveryFailure {
  return {
    ok: false,
    agentId,
    reason,
    retryable: true,
  };
}

function protocolErrorCode(error: unknown): ServerProtocolErrorCode {
  if (!(error instanceof ProtocolError)) {
    return 'invalid-message';
  }

  switch (error.code) {
    case 'message_too_large':
      return 'message-too-large';
    case 'unsupported_protocol_version':
      return 'unsupported-protocol-version';
    case 'invalid_json':
    case 'invalid_message':
      return 'invalid-message';
  }

  return 'invalid-message';
}
