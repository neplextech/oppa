import { randomUUID } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';

import {
  decodeAgentMessage,
  encodeServerMessage,
  PROTOCOL_VERSION,
  ProtocolError,
  type AgentMessage,
  type PrintJob,
  type PrinterDescriptor,
} from '@openprinter/protocol';

import {
  boundedDetail,
  transportCloseRequest,
  utf8ByteLength,
  type ResolvedOpenPrinterServerOptions,
} from './internal.js';
import type {
  AcceptOpenPrinterSessionInput,
  AgentDisconnectReason,
  AgentMessageOf,
  CallbackErrorEvent,
  ConfigurationInvalidation,
  ConnectedAgent,
  DeliveryFailure,
  DeliveryResult,
  DisconnectAgentOptions,
  JobCancellation,
  OpenPrinterApplicationMessage,
  OpenPrinterCallbackName,
  OpenPrinterServerOptions,
  OpenPrinterSession,
  OpenPrinterSessionState,
  OpenPrinterTransport,
  OpenPrinterTransportClosedEvent,
  PrintersChangedEvent,
  ServerMessageOf,
  ServerProtocolErrorCode,
} from './types.js';

interface TransportWriteSuccess {
  readonly ok: true;
}

interface TransportWriteFailure {
  readonly ok: false;
  readonly error: unknown;
}

type TransportWriteResult = TransportWriteSuccess | TransportWriteFailure;

class HostCallbackTimeoutError extends Error {
  public constructor(callback: OpenPrinterCallbackName) {
    super(`The ${callback} host callback exceeded the configured lifecycle callback timeout.`);
    this.name = 'HostCallbackTimeoutError';
  }
}

class TransportCallbackTimeoutError extends Error {
  public constructor(operation: 'send' | 'close') {
    super(`The transport ${operation} callback exceeded the configured timeout.`);
    this.name = 'TransportCallbackTimeoutError';
  }
}

/** @internal */
export class OpenPrinterSessionImplementation<Metadata> implements OpenPrinterSession<Metadata> {
  public readonly identity: AcceptOpenPrinterSessionInput<Metadata>['identity'];
  public readonly sessionId: string;

  readonly #options: OpenPrinterServerOptions<Metadata>;
  readonly #resolved: ResolvedOpenPrinterServerOptions;
  readonly #transport: OpenPrinterTransport;
  readonly #printers = new Map<string, PrinterDescriptor>();
  readonly #pendingHeartbeats = new Map<string, number>();
  #state: OpenPrinterSessionState = 'handshaking';
  #hello: AgentMessageOf<'agent.hello'>['payload'] | null = null;
  #connectedAtMs: number | null = null;
  #lastSeenAtMs = Date.now();
  #lastHeartbeatAtMs = this.#lastSeenAtMs;
  #printerRevision: number | null = null;
  #handshakeTimer: ReturnType<typeof setTimeout> | null = null;
  #heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  #heartbeatDeadline: ReturnType<typeof setTimeout> | null = null;
  #processing: Promise<void> = Promise.resolve();
  #outbound: Promise<void> = Promise.resolve();
  #disconnectReason: AgentDisconnectReason | null = null;
  #transportCloseRequested = false;

  public constructor(
    options: OpenPrinterServerOptions<Metadata>,
    resolved: ResolvedOpenPrinterServerOptions,
    input: AcceptOpenPrinterSessionInput<Metadata>,
    sessionId: string,
  ) {
    this.#options = options;
    this.#resolved = resolved;
    this.identity = input.identity;
    this.sessionId = sessionId;
    this.#transport = input.transport;

    this.#handshakeTimer = setTimeout(() => {
      void this.#enqueue(() =>
        this.#protocolViolation(
          'handshake-timeout',
          new Error('The agent did not send agent.hello before the timeout.'),
        ),
      );
    }, resolved.handshakeTimeoutMs);
    unrefTimer(this.#handshakeTimer);
  }

  public get state(): OpenPrinterSessionState {
    return this.#state;
  }

  public getAgent(): ConnectedAgent<Metadata> | null {
    return this.#hello === null || this.#connectedAtMs === null ? null : this.#snapshot();
  }

  public getPrinters(): readonly PrinterDescriptor[] {
    return structuredClone([...this.#printers.values()]);
  }

  public receive(message: string | Uint8Array): Promise<void> {
    return this.#enqueue(() => this.#receiveMessage(message));
  }

  public transportClosed(event: OpenPrinterTransportClosedEvent = {}): Promise<void> {
    return this.#enqueue(() => this.#handleTransportClosed(event));
  }

  public async send(message: OpenPrinterApplicationMessage): Promise<DeliveryResult> {
    const encoded = encodeServerMessage(message);
    const encodedBytes = utf8ByteLength(encoded);
    if (encodedBytes > this.#resolved.maxMessageBytes) {
      throw new ProtocolError(
        'message_too_large',
        `The encoded server message is ${encodedBytes} bytes; this session's configured limit is ${this.#resolved.maxMessageBytes} bytes.`,
      );
    }

    if (this.#state === 'handshaking') {
      return this.#deliveryFailure('session-not-ready');
    }
    if (this.#state !== 'connected') {
      return this.#deliveryFailure('connection-closed');
    }

    const result = await this.#writeEncoded(encoded, ['connected']);
    if (!result.ok) {
      await this.#transportFailed(result.error);
      return this.#deliveryFailure('transport-error');
    }

    return {
      ok: true,
      agentId: this.identity.agentId,
      sessionId: this.sessionId,
      messageId: message.messageId,
      sentAt: message.sentAt,
    };
  }

  public sendJob(job: PrintJob): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.print_job'> = {
      ...newEnvelope(),
      type: 'server.print_job',
      payload: job,
    };
    return this.send(message);
  }

  public requestPrinters(): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.request_printer_inventory'> = {
      ...newEnvelope(),
      type: 'server.request_printer_inventory',
      payload: {},
    };
    return this.send(message);
  }

  public cancelJob(cancellation: JobCancellation): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.cancel_job'> = {
      ...newEnvelope(),
      type: 'server.cancel_job',
      payload: cancellation,
    };
    return this.send(message);
  }

  public invalidateConfiguration(invalidation: ConfigurationInvalidation): Promise<DeliveryResult> {
    const message: ServerMessageOf<'server.configuration_invalidated'> = {
      ...newEnvelope(),
      type: 'server.configuration_invalidated',
      payload: invalidation,
    };
    return this.send(message);
  }

  public async disconnect(options: DisconnectAgentOptions = {}): Promise<boolean> {
    if (this.#state === 'closing' || this.#state === 'closed') {
      return false;
    }

    const wasConnected = this.#state === 'connected';
    this.#state = 'closing';
    this.#disconnectReason = 'server-disconnect';

    if (wasConnected) {
      const message = createDisconnectMessage({
        code: options.code ?? 'server_disconnect',
        reason: options.reason ?? 'The host application closed this connection.',
        reconnect: options.reconnect ?? true,
        ...(options.retryAfterMs === undefined ? {} : { retryAfterMs: options.retryAfterMs }),
      });
      await this.#writeEncoded(encodeServerMessage(message), ['closing']);
    }

    await this.#requestTransportClose(
      transportCloseRequest('server-disconnect', options.reason ?? 'Server disconnect'),
    );
    await this.#finalize('server-disconnect');
    return true;
  }

  #enqueue(operation: () => Promise<void>): Promise<void> {
    const queued = this.#processing.then(async () => {
      try {
        await operation();
      } catch (error) {
        await this.#protocolViolation(
          'invalid-message',
          error instanceof Error ? error : new Error('Unexpected message processing failure.'),
        );
      }
    });
    this.#processing = queued.catch(() => {
      // All expected failures are converted into protocol/transport lifecycle
      // events. Keep the queue usable if a defensive finalizer also fails.
    });
    return this.#processing;
  }

  async #receiveMessage(input: string | Uint8Array): Promise<void> {
    if (this.#state === 'closing' || this.#state === 'closed') {
      return;
    }

    const bytes = typeof input === 'string' ? utf8ByteLength(input) : input.byteLength;
    if (bytes > this.#resolved.maxMessageBytes) {
      await this.#protocolViolation(
        'message-too-large',
        new Error(`The inbound message exceeded ${this.#resolved.maxMessageBytes} bytes.`),
      );
      return;
    }

    let message: AgentMessage;
    try {
      message = decodeAgentMessage(input);
    } catch (error) {
      const protocolError = error instanceof Error ? error : new Error('The agent message was invalid.');
      await this.#protocolViolation(protocolErrorCode(error), protocolError);
      return;
    }

    this.#lastSeenAtMs = Date.now();

    if (this.#state === 'handshaking') {
      if (message.type !== 'agent.hello') {
        await this.#protocolViolation('unexpected-message', new Error('The first agent message must be agent.hello.'));
        return;
      }

      await this.#completeHandshake(message);
      return;
    }

    if (message.type === 'agent.hello') {
      await this.#protocolViolation('unexpected-message', new Error('agent.hello may only be sent once per session.'));
      return;
    }

    await this.#routeMessage(message);
  }

  async #completeHandshake(hello: AgentMessageOf<'agent.hello'>): Promise<void> {
    if (hello.payload.agentId !== this.identity.agentId) {
      await this.#protocolViolation(
        'identity-mismatch',
        new Error('The host-authenticated identity does not match the agent hello.'),
      );
      return;
    }

    if (!hello.payload.supportedProtocolVersions.includes(PROTOCOL_VERSION)) {
      await this.#protocolViolation(
        'unsupported-protocol-version',
        new Error('The peers do not share a supported protocol version.'),
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
        brand: this.#resolved.brand,
        sessionId: this.sessionId,
        supportedProtocolVersions: [PROTOCOL_VERSION],
        selectedProtocolVersion: PROTOCOL_VERSION,
        heartbeatIntervalMs: this.#resolved.heartbeatIntervalMs,
        maxMessageBytes: this.#resolved.maxMessageBytes,
      },
    };

    const handoff = await this.#writeEncoded(encodeServerMessage(response), ['handshaking']);
    if (!handoff.ok) {
      await this.#transportFailed(handoff.error);
      return;
    }

    this.#clearHandshakeTimer();
    const now = Date.now();
    this.#hello = hello.payload;
    this.#connectedAtMs = now;
    this.#lastHeartbeatAtMs = now;
    this.#state = 'connected';
    this.#startHeartbeats();

    await this.#invoke('onAgentConnected', this.#options.onAgentConnected, {
      session: this,
      agent: this.#snapshot(),
    });
    await this.#sendHeartbeat();
  }

  async #routeMessage(message: Exclude<AgentMessage, AgentMessageOf<'agent.hello'>>): Promise<void> {
    switch (message.type) {
      case 'agent.authentication_metadata':
        await this.#invoke('onAuthenticationMetadata', this.#options.onAuthenticationMetadata, {
          session: this,
          agent: this.#snapshot(),
          message,
        });
        return;

      case 'agent.heartbeat':
        if (!this.#pendingHeartbeats.has(message.correlationId)) {
          await this.#protocolViolation(
            'unexpected-message',
            new Error('The heartbeat does not correlate to an outstanding request.'),
          );
          return;
        }

        this.#pendingHeartbeats.delete(message.correlationId);
        this.#lastHeartbeatAtMs = Date.now();
        this.#prunePendingHeartbeats();
        this.#armHeartbeatDeadline();
        return;

      case 'agent.printer_inventory':
        if (this.#printerRevision !== null && message.payload.revision < this.#printerRevision) {
          await this.#protocolViolation('unexpected-message', new Error('The printer inventory revision regressed.'));
          return;
        }
        if (
          this.#printerRevision === message.payload.revision &&
          samePrinterInventory(this.#printers, message.payload.printers)
        ) {
          return;
        }
        if (this.#printerRevision === message.payload.revision) {
          await this.#protocolViolation(
            'unexpected-message',
            new Error('The printer inventory changed without advancing its revision.'),
          );
          return;
        }
        this.#printers.clear();
        for (const printer of message.payload.printers) {
          this.#printers.set(printer.id, printer);
        }
        this.#printerRevision = message.payload.revision;
        await this.#printersChanged('snapshot', message);
        return;

      case 'agent.printer_inventory_changed': {
        const inventoryError = inventoryChangeError(this.#printerRevision, this.#printers, message);
        if (inventoryError !== null) {
          await this.#protocolViolation('unexpected-message', new Error(inventoryError));
          return;
        }

        for (const printerId of message.payload.removedPrinterIds) {
          this.#printers.delete(printerId);
        }
        for (const printer of message.payload.added) {
          this.#printers.set(printer.id, printer);
        }
        for (const printer of message.payload.updated) {
          this.#printers.set(printer.id, printer);
        }
        this.#printerRevision = message.payload.revision;
        await this.#printersChanged('change', message);
        return;
      }

      case 'agent.job_received':
        await this.#invoke('onJobReceived', this.#options.onJobReceived, {
          session: this,
          agent: this.#snapshot(),
          message,
        });
        return;

      case 'agent.job_submitted':
        await this.#invoke('onJobSubmitted', this.#options.onJobSubmitted, {
          session: this,
          agent: this.#snapshot(),
          message,
        });
        return;

      case 'agent.job_failed':
        await this.#invoke('onJobFailed', this.#options.onJobFailed, {
          session: this,
          agent: this.#snapshot(),
          message,
        });
        return;

      case 'agent.diagnostics':
        if (message.payload.agentId !== this.identity.agentId) {
          await this.#protocolViolation(
            'identity-mismatch',
            new Error('The diagnostics identity does not match the host-authenticated agent.'),
          );
          return;
        }

        await this.#invoke('onDiagnostics', this.#options.onDiagnostics, {
          session: this,
          agent: this.#snapshot(),
          message,
        });
    }
  }

  async #printersChanged(
    kind: PrintersChangedEvent<Metadata>['kind'],
    message: AgentMessageOf<'agent.printer_inventory'> | AgentMessageOf<'agent.printer_inventory_changed'>,
  ): Promise<void> {
    await this.#invoke('onPrintersChanged', this.#options.onPrintersChanged, {
      session: this,
      agent: this.#snapshot(),
      kind,
      revision: message.payload.revision,
      printers: this.getPrinters(),
      message,
    });
  }

  #startHeartbeats(): void {
    this.#heartbeatTimer = setInterval(() => {
      void this.#sendHeartbeat();
    }, this.#resolved.heartbeatIntervalMs);
    unrefTimer(this.#heartbeatTimer);
    this.#armHeartbeatDeadline();
  }

  async #sendHeartbeat(): Promise<void> {
    if (this.#state !== 'connected') {
      return;
    }

    this.#prunePendingHeartbeats();
    const message: ServerMessageOf<'server.heartbeat'> = {
      ...newEnvelope(),
      type: 'server.heartbeat',
      payload: {
        timeoutMs: this.#resolved.heartbeatTimeoutMs,
      },
    };
    this.#pendingHeartbeats.set(message.messageId, Date.now());
    const result = await this.#writeEncoded(encodeServerMessage(message), ['connected']);

    if (!result.ok) {
      this.#pendingHeartbeats.delete(message.messageId);
      await this.#transportFailed(result.error);
    }
  }

  #armHeartbeatDeadline(): void {
    if (this.#heartbeatDeadline !== null) {
      clearTimeout(this.#heartbeatDeadline);
    }

    this.#heartbeatDeadline = setTimeout(() => {
      void this.#heartbeatExpired();
    }, this.#resolved.heartbeatTimeoutMs);
    unrefTimer(this.#heartbeatDeadline);
  }

  async #heartbeatExpired(): Promise<void> {
    if (this.#state !== 'connected') {
      return;
    }

    const timeoutEvent = {
      session: this,
      agent: this.#snapshot(),
      lastHeartbeatAt: new Date(this.#lastHeartbeatAtMs).toISOString(),
      timeoutMs: this.#resolved.heartbeatTimeoutMs,
    };
    this.#state = 'closing';
    this.#disconnectReason = 'heartbeat-timeout';

    const message = createDisconnectMessage({
      code: 'heartbeat_timeout',
      reason: 'The agent did not answer protocol heartbeats in time.',
      reconnect: true,
      retryAfterMs: this.#resolved.heartbeatIntervalMs,
    });
    await this.#writeEncoded(encodeServerMessage(message), ['closing']);
    await this.#requestTransportClose(transportCloseRequest('heartbeat-timeout', 'Heartbeat timeout'));
    await this.#finalize('heartbeat-timeout');
    await this.#invoke('onHeartbeatTimeout', this.#options.onHeartbeatTimeout, timeoutEvent);
  }

  #prunePendingHeartbeats(): void {
    const cutoff = Date.now() - this.#resolved.heartbeatTimeoutMs;
    for (const [messageId, sentAt] of this.#pendingHeartbeats) {
      if (sentAt < cutoff) {
        this.#pendingHeartbeats.delete(messageId);
      }
    }
  }

  async #protocolViolation(code: ServerProtocolErrorCode, error: Error): Promise<void> {
    if (this.#state === 'closed' || this.#state === 'closing') {
      return;
    }

    const agent = this.#state === 'connected' ? this.#snapshot() : undefined;
    this.#state = 'closing';
    this.#disconnectReason = 'protocol-error';
    const event = {
      session: this,
      agentId: this.identity.agentId,
      ...(agent === undefined ? {} : { agent }),
      code,
      error,
    };

    if (agent !== undefined) {
      const message = createDisconnectMessage({
        code: 'protocol_error',
        reason: 'The connection violated the OpenPrinter protocol.',
        reconnect: false,
      });
      await this.#writeEncoded(encodeServerMessage(message), ['closing']);
    }

    await this.#requestTransportClose(transportCloseRequest('protocol-error', 'Protocol error'));
    await this.#finalize('protocol-error');
    await this.#invoke('onProtocolError', this.#options.onProtocolError, event);
  }

  async #handleTransportClosed(event: OpenPrinterTransportClosedEvent): Promise<void> {
    if (this.#state === 'closed') {
      return;
    }

    const reason = this.#disconnectReason ?? event.reason ?? 'peer-closed';
    await this.#finalize(reason, event.detail);
  }

  async #transportFailed(error: unknown): Promise<void> {
    if (this.#state === 'closed') {
      return;
    }

    const detail = error instanceof Error ? error.name : 'UnknownTransportError';
    this.#state = 'closing';
    this.#disconnectReason = 'transport-error';
    await this.#requestTransportClose(transportCloseRequest('transport-error', detail));
    await this.#finalize('transport-error', detail);
  }

  async #finalize(reason: AgentDisconnectReason, detail?: string): Promise<void> {
    if (this.#state === 'closed') {
      return;
    }

    const snapshot = this.#hello === null || this.#connectedAtMs === null ? null : this.#snapshot();
    this.#state = 'closed';
    this.#disconnectReason = reason;
    this.#clearTimers();
    this.#pendingHeartbeats.clear();

    if (snapshot !== null) {
      const bounded = detail === undefined ? undefined : boundedDetail(detail);
      await this.#invoke('onAgentDisconnected', this.#options.onAgentDisconnected, {
        session: this,
        agent: snapshot,
        reason,
        ...(bounded === undefined ? {} : { detail: bounded }),
      });
    }
  }

  async #writeEncoded(
    encoded: string,
    allowedStates: readonly OpenPrinterSessionState[],
  ): Promise<TransportWriteResult> {
    if (!allowedStates.includes(this.#state)) {
      return {
        ok: false,
        error: new Error('The protocol session is not writable.'),
      };
    }

    let result: TransportWriteResult = {
      ok: false,
      error: new Error('The protocol session is not writable.'),
    };

    const operation = this.#outbound.then(async () => {
      try {
        await this.#boundedTransport('send', () => this.#transport.send(encoded));
        result = { ok: true };
      } catch (error) {
        result = { ok: false, error };
      }
    });
    this.#outbound = operation.catch(() => {
      // The operation body converts transport exceptions into `result`.
    });
    await this.#outbound;
    return result;
  }

  async #requestTransportClose(request: Parameters<OpenPrinterTransport['close']>[0]): Promise<void> {
    if (this.#transportCloseRequested) {
      return;
    }
    this.#transportCloseRequested = true;

    try {
      await this.#boundedTransport('close', () => this.#transport.close(request));
    } catch {
      // The protocol state must finish closing even if a host transport cannot
      // acknowledge cleanup. The host already owns that transport lifecycle.
    }
  }

  async #boundedTransport(operation: 'send' | 'close', callback: () => unknown): Promise<void> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_resolve, reject) => {
      timer = setTimeout(() => {
        reject(new TransportCallbackTimeoutError(operation));
      }, this.#resolved.transportTimeoutMs);
      unrefTimer(timer);
    });

    try {
      await Promise.race([Promise.resolve().then(callback), timeout]);
    } finally {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
    }
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
      unrefTimer(timer);
    });

    try {
      await Promise.race([Promise.resolve().then(() => callback(event)), timeout]);
    } finally {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
    }
  }

  #snapshot(): ConnectedAgent<Metadata> {
    if (this.#hello === null || this.#connectedAtMs === null) {
      throw new Error('Cannot expose a session before the protocol handshake.');
    }

    return {
      agentId: this.identity.agentId,
      sessionId: this.sessionId,
      ...(this.identity.metadata === undefined ? {} : { metadata: this.identity.metadata }),
      hello: structuredClone(this.#hello),
      connectedAt: new Date(this.#connectedAtMs).toISOString(),
      lastSeenAt: new Date(this.#lastSeenAtMs).toISOString(),
      printerRevision: this.#printerRevision,
    };
  }

  #deliveryFailure(reason: DeliveryFailure['reason']): DeliveryFailure {
    return {
      ok: false,
      agentId: this.identity.agentId,
      reason,
      retryable: true,
    };
  }

  #clearHandshakeTimer(): void {
    if (this.#handshakeTimer !== null) {
      clearTimeout(this.#handshakeTimer);
      this.#handshakeTimer = null;
    }
  }

  #clearTimers(): void {
    this.#clearHandshakeTimer();
    if (this.#heartbeatTimer !== null) {
      clearInterval(this.#heartbeatTimer);
      this.#heartbeatTimer = null;
    }
    if (this.#heartbeatDeadline !== null) {
      clearTimeout(this.#heartbeatDeadline);
      this.#heartbeatDeadline = null;
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

function inventoryChangeError(
  printerRevision: number | null,
  printers: ReadonlyMap<string, PrinterDescriptor>,
  message: AgentMessageOf<'agent.printer_inventory_changed'>,
): string | null {
  if (printerRevision === null) {
    return 'An incremental printer inventory requires an initial snapshot.';
  }
  if (message.payload.revision <= printerRevision) {
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
  if ([...added].some((id) => printers.has(id))) {
    return 'The incremental printer inventory adds an existing printer.';
  }
  if ([...updated, ...removed].some((id) => !printers.has(id))) {
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
}

function unrefTimer(timer: ReturnType<typeof setTimeout>): void {
  timer.unref();
}
