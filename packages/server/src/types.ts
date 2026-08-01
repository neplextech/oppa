import type {
  AgentMessage,
  GatewayAuthenticationFailureCode,
  OpenPrinterDiscoveryDocument,
  OpenPrinterErrorEnvelope,
  OpenPrinterPairingResponse,
  OpenPrinterBrandMetadata,
  PublicEd25519Jwk,
  PrintJob,
  PrinterDescriptor,
  ServerMessage,
} from '@openprinter/protocol';

/** A value or promise accepted by transport and lifecycle callbacks. */
export type Awaitable<Value> = Value | PromiseLike<Value>;

/** Select one canonical agent message using its wire discriminator. */
export type AgentMessageOf<Type extends AgentMessage['type']> = Extract<AgentMessage, { readonly type: Type }>;

/** Select one canonical server message using its wire discriminator. */
export type ServerMessageOf<Type extends ServerMessage['type']> = Extract<ServerMessage, { readonly type: Type }>;

/**
 * Messages an application may send after the SDK owns hello, heartbeat, and
 * disconnect protocol control.
 */
export type OpenPrinterApplicationMessage = Exclude<
  ServerMessage,
  ServerMessageOf<'server.hello'> | ServerMessageOf<'server.heartbeat'> | ServerMessageOf<'server.disconnect'>
>;

/** Identity established from a successfully verified paired public credential. */
export interface AuthenticatedAgent<Metadata> {
  /** Stable identity registered with the verified public credential. */
  readonly agentId: string;
  /** Host-owned, non-secret context retained with this session. */
  readonly metadata?: Metadata;
}

/** Immutable public snapshot of an authenticated, negotiated agent session. */
export interface ConnectedAgent<Metadata> {
  /** Stable identity registered with the verified public credential. */
  readonly agentId: string;
  /** Unique identifier for this logical transport session. */
  readonly sessionId: string;
  /** Host-owned metadata supplied when the session was accepted. */
  readonly metadata?: Readonly<Metadata>;
  /** Validated agent hello payload. */
  readonly hello: Readonly<AgentMessageOf<'agent.hello'>['payload']>;
  /** UTC time at which the protocol handshake completed. */
  readonly connectedAt: string;
  /** UTC time of the most recently validated agent message. */
  readonly lastSeenAt: string;
  /** Latest full or incremental inventory revision, when one was received. */
  readonly printerRevision: number | null;
}

/** Lifecycle state of one accepted protocol session. */
export type OpenPrinterSessionState = 'authenticating' | 'handshaking' | 'connected' | 'closing' | 'closed';

/** Why a negotiated agent session ended. */
export type AgentDisconnectReason =
  | 'peer-closed'
  | 'authentication-failed'
  | 'heartbeat-timeout'
  | 'protocol-error'
  | 'server-disconnect'
  | 'transport-error';

/**
 * SDK request for the host to close its transport.
 *
 * The reason is transport-neutral. A WebSocket host may map it to an
 * appropriate RFC 6455 close code; a broker transport may release a consumer
 * or route instead.
 */
export interface OpenPrinterTransportCloseRequest {
  /** Stable lifecycle reason suitable for transport-specific mapping. */
  readonly reason: Exclude<AgentDisconnectReason, 'peer-closed'>;
  /** Bounded, non-sensitive human-readable detail. */
  readonly detail?: string;
}

/**
 * Host-owned transport callbacks for one logical agent connection.
 *
 * Resolving `send` means the frame was handed to the transport, not that the
 * agent persisted or printed its contents.
 */
export interface OpenPrinterTransport {
  /** Hand one encoded UTF-8 JSON protocol message to the host transport. */
  send(message: string): Awaitable<void>;
  /** Close or release the host transport for an SDK-requested reason. */
  close(request: OpenPrinterTransportCloseRequest): Awaitable<void>;
}

/** Host notification that its transport has already ended. */
export interface OpenPrinterTransportClosedEvent {
  /** Whether the peer closed normally or the transport itself failed. */
  readonly reason?: 'peer-closed' | 'transport-error';
  /** Bounded, sanitized detail that contains no credentials or documents. */
  readonly detail?: string;
}

/** Input used to open one transport that the SDK will authenticate. */
export interface AcceptOpenPrinterSessionInput {
  /** Optional host-assigned logical session identifier. */
  readonly sessionId?: string;
  /** Host-owned connection, socket, broker, or other message transport. */
  readonly transport: OpenPrinterTransport;
}

/** Opaque metadata bound to a one-time pairing grant by the host application. */
export interface PairingCodeRecord<Metadata> {
  readonly code: string;
  readonly createdAt: Date;
  readonly expiresAt: Date;
  readonly metadata?: Metadata;
}

/** Input persisted by a pairing-code store. */
export type CreatePairingCodeStoreInput<Metadata> = PairingCodeRecord<Metadata>;

/** Result of atomically trying to consume a pairing code. */
export type PairingCodeConsumeResult = 'consumed' | 'expired' | 'invalid' | 'already-consumed';

/** Storage boundary for short-lived, single-use pairing grants. */
export interface PairingCodeStore<Metadata> {
  create(input: CreatePairingCodeStoreInput<Metadata>): Promise<PairingCodeRecord<Metadata>>;
  consume(
    code: string,
    now: Date,
    register: (record: PairingCodeRecord<Metadata>) => Promise<void>,
  ): Promise<PairingCodeConsumeResult>;
}

/** Public credential registered for one paired agent. */
export interface AgentCredentialRecord<Metadata> {
  readonly agentId: string;
  readonly keyId: string;
  readonly algorithm: 'Ed25519';
  readonly publicKey: PublicEd25519Jwk;
  readonly createdAt: Date;
  readonly revokedAt: Date | null;
  readonly metadata?: Metadata;
}

/** Credential persistence boundary owned by the host application. */
export interface AgentCredentialStore<Metadata> {
  create(input: AgentCredentialRecord<Metadata>): Promise<AgentCredentialRecord<Metadata>>;
  find(agentId: string, keyId: string): Promise<AgentCredentialRecord<Metadata> | null>;
  revoke(agentId: string, keyId: string, revokedAt: Date): Promise<void>;
}

/** Input used by the host to create a temporary pairing code. */
export interface CreatePairingCodeInput<Metadata> {
  readonly expiresInMs?: number;
  readonly metadata?: Metadata;
}

/** Temporary code returned only to the host application that created it. */
export type CreatedPairingCode<Metadata> = PairingCodeRecord<Metadata>;

/** Optional request context passed to pairing rate-limit policy. */
export interface PairingAttemptContext {
  readonly remoteAddress?: string;
}

/** Pairing rate-limit hook result. */
export interface PairingRateLimitResult {
  readonly allowed: boolean;
  readonly retryAfterMs?: number;
}

/** Structural WebSocket used only by the optional convenience adapter. */
export interface OpenPrinterGatewaySocket {
  send(data: string, callback?: (error?: Error) => void): unknown;
  close(code?: number, reason?: string): unknown;
  on(event: 'message', listener: (data: unknown) => void): unknown;
  on(event: 'close', listener: (code?: number, reason?: unknown) => void): unknown;
  on(event: 'error', listener: (error: Error) => void): unknown;
}

/** Common context for a validated message from an authenticated agent. */
export interface AgentMessageEvent<Metadata, Message extends AgentMessage> {
  /** Protocol session that received the message. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Snapshot of the agent at callback dispatch time. */
  readonly agent: ConnectedAgent<Metadata>;
  /** Canonical, runtime-validated protocol message. */
  readonly message: Message;
}

/** A completed agent handshake. */
export interface AgentConnectedEvent<Metadata> {
  /** Protocol session ready for application commands. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Newly negotiated agent snapshot. */
  readonly agent: ConnectedAgent<Metadata>;
}

/** A connected agent session that has ended. */
export interface AgentDisconnectedEvent<Metadata> {
  /** Session that ended. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Final snapshot of the disconnected agent. */
  readonly agent: ConnectedAgent<Metadata>;
  /** Stable lifecycle reason independent of transport wording. */
  readonly reason: AgentDisconnectReason;
  /** Bounded, non-sensitive transport detail. */
  readonly detail?: string;
}

/** Inventory event emitted after the in-memory session view is updated. */
export interface PrintersChangedEvent<Metadata> {
  /** Protocol session that owns the inventory. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Agent that owns the inventory. */
  readonly agent: ConnectedAgent<Metadata>;
  /** Whether this message replaced or incrementally changed the inventory. */
  readonly kind: 'snapshot' | 'change';
  /** Validated revision reported by the agent. */
  readonly revision: number;
  /** Complete current inventory after applying the message. */
  readonly printers: readonly PrinterDescriptor[];
  /** Canonical inventory message that caused the event. */
  readonly message: AgentMessageOf<'agent.printer_inventory'> | AgentMessageOf<'agent.printer_inventory_changed'>;
}

/** An authenticated session that did not answer protocol heartbeats in time. */
export interface HeartbeatTimeoutEvent<Metadata> {
  /** Timed-out protocol session. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Timed-out agent. */
  readonly agent: ConnectedAgent<Metadata>;
  /** UTC time of its latest correlated heartbeat response. */
  readonly lastHeartbeatAt: string;
  /** Configured response timeout. */
  readonly timeoutMs: number;
}

/** Stable categories for rejected agent protocol behavior. */
export type ServerProtocolErrorCode =
  | GatewayAuthenticationFailureCode
  | 'handshake-timeout'
  | 'identity-mismatch'
  | 'invalid-message'
  | 'message-too-large'
  | 'unexpected-message'
  | 'unsupported-protocol-version';

/** A payload-free protocol failure from one accepted session. */
export interface ServerProtocolErrorEvent<Metadata> {
  /** Session that rejected the input. */
  readonly session: OpenPrinterSession<Metadata>;
  /** Authenticated identity, even when the hello was not completed. */
  readonly agentId: string;
  /** Connected snapshot when the handshake had completed. */
  readonly agent?: ConnectedAgent<Metadata>;
  /** Stable protocol error category. */
  readonly code: ServerProtocolErrorCode;
  /** Error object without the rejected payload. */
  readonly error: Error;
}

/** Names of user callbacks isolated by the SDK. */
export type OpenPrinterCallbackName =
  | 'onAgentConnected'
  | 'onAgentDisconnected'
  | 'onCallbackError'
  | 'onDiagnostics'
  | 'onHeartbeatTimeout'
  | 'onJobFailed'
  | 'onJobReceived'
  | 'onJobSubmitted'
  | 'onPrintersChanged'
  | 'onProtocolError';

/** A host callback that rejected or threw after SDK state was made consistent. */
export interface CallbackErrorEvent {
  /** Callback whose invocation failed. */
  readonly callback: Exclude<OpenPrinterCallbackName, 'onCallbackError'>;
  /** Original thrown value. */
  readonly error: unknown;
}

/** Configurable protocol behavior and lifecycle callbacks shared by sessions. */
export interface OpenPrinterServerOptions<Metadata> {
  /** Required display identity sent to the agent in `server.hello`. */
  readonly brand: OpenPrinterBrandMetadata;
  /** Stable server identifier advertised during the handshake. */
  readonly serverId?: string;
  /** Human-readable server SDK or host version advertised to agents. */
  readonly serverVersion?: string;
  /** Framework-independent endpoint path configuration. */
  readonly paths?: Partial<OpenPrinterServerPaths>;
  /** Storage for temporary pairing grants; defaults to an in-memory implementation. */
  readonly pairingCodeStore?: PairingCodeStore<Metadata>;
  /** Storage for agent public keys; defaults to an in-memory implementation. */
  readonly credentialStore?: AgentCredentialStore<Metadata>;
  /** Time allowed for the initial challenge response. */
  readonly authenticationTimeoutMs?: number;
  /** Socket-bound challenge lifetime. */
  readonly challengeTtlMs?: number;
  /** Optional application rate-limit policy for pairing attempts. */
  readonly checkPairingRateLimit?: (context: PairingAttemptContext) => Awaitable<PairingRateLimitResult | boolean>;
  /** Maximum accepted encoded message size in bytes. */
  readonly maxMessageBytes?: number;
  /** Time allowed for the first validated `agent.hello`. */
  readonly handshakeTimeoutMs?: number;
  /** Maximum time allowed for a transport `send` or `close` callback. */
  readonly transportTimeoutMs?: number;
  /** Maximum time allowed for any lifecycle callback before it is isolated. */
  readonly callbackTimeoutMs?: number;
  /** Frequency of server heartbeat requests. */
  readonly heartbeatIntervalMs?: number;
  /** Maximum time between correlated heartbeat responses. */
  readonly heartbeatTimeoutMs?: number;
  /** Called after a session is negotiated and ready for commands. */
  readonly onAgentConnected?: (event: AgentConnectedEvent<Metadata>) => Awaitable<void>;
  /** Called exactly once for a session whose handshake had completed. */
  readonly onAgentDisconnected?: (event: AgentDisconnectedEvent<Metadata>) => Awaitable<void>;
  /** Called after a complete or incremental inventory is applied. */
  readonly onPrintersChanged?: (event: PrintersChangedEvent<Metadata>) => Awaitable<void>;
  /** Called after the agent durably persists a delivered job. */
  readonly onJobReceived?: (
    event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_received'>>,
  ) => Awaitable<void>;
  /** Called after a printer backend accepts a job for submission. */
  readonly onJobSubmitted?: (
    event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_submitted'>>,
  ) => Awaitable<void>;
  /** Called after the agent reports a recoverable or terminal job failure. */
  readonly onJobFailed?: (event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.job_failed'>>) => Awaitable<void>;
  /** Called for a bounded, validated agent diagnostic summary. */
  readonly onDiagnostics?: (event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.diagnostics'>>) => Awaitable<void>;
  /** Called after a heartbeat-expired session has been made unavailable. */
  readonly onHeartbeatTimeout?: (event: HeartbeatTimeoutEvent<Metadata>) => Awaitable<void>;
  /** Called when an inbound protocol invariant is rejected. */
  readonly onProtocolError?: (event: ServerProtocolErrorEvent<Metadata>) => Awaitable<void>;
  /** Called when another user-provided lifecycle callback throws or rejects. */
  readonly onCallbackError?: (event: CallbackErrorEvent) => Awaitable<void>;
}

/** Successful immediate handoff to a host-owned transport. */
export interface DeliverySuccess {
  /** Distinguishes successful delivery results. */
  readonly ok: true;
  /** Agent selected by the host application. */
  readonly agentId: string;
  /** Logical session that accepted the handoff. */
  readonly sessionId: string;
  /** Canonical wire message identifier used for correlation. */
  readonly messageId: string;
  /** UTC time encoded in the delivered protocol message. */
  readonly sentAt: string;
}

/** Reasons a message could not be handed to an agent transport. */
export type DeliveryFailureReason = 'agent-offline' | 'session-not-ready' | 'connection-closed' | 'transport-error';

/** Structured, non-queued delivery failure owned by the host application. */
export interface DeliveryFailure {
  /** Distinguishes failed delivery results. */
  readonly ok: false;
  /** Requested agent identity. */
  readonly agentId: string;
  /** Stable reason suitable for application queue policy. */
  readonly reason: DeliveryFailureReason;
  /** Signals that a later application-owned retry may succeed. */
  readonly retryable: true;
}

/** Immediate transport handoff result; it is not a persistence acknowledgement. */
export type DeliveryResult = DeliverySuccess | DeliveryFailure;

/** Host-supplied reason for intentionally disconnecting one session. */
export interface DisconnectAgentOptions {
  /** Stable identifier delivered to the agent. */
  readonly code?: string;
  /** Human-readable reason delivered to the agent. */
  readonly reason?: string;
  /** Whether the agent should reconnect. */
  readonly reconnect?: boolean;
  /** Optional delay before reconnection. */
  readonly retryAfterMs?: number;
}

/** Host-supplied configuration invalidation payload. */
export type ConfigurationInvalidation = ServerMessageOf<'server.configuration_invalidated'>['payload'];

/** Host-supplied cancellation payload. */
export type JobCancellation = ServerMessageOf<'server.cancel_job'>['payload'];

/** One host-authenticated OpenPrinter protocol session. */
export interface OpenPrinterSession<Metadata> {
  /** Authenticated identity, or `null` until challenge verification succeeds. */
  readonly identity: AuthenticatedAgent<Metadata> | null;
  /** Stable logical session identifier. */
  readonly sessionId: string;
  /** Current protocol lifecycle state. */
  readonly state: OpenPrinterSessionState;
  /** Return a negotiated agent snapshot, or `null` during/after a failed handshake. */
  getAgent(): ConnectedAgent<Metadata> | null;
  /** Return the latest validated printer inventory for this session. */
  getPrinters(): readonly PrinterDescriptor[];
  /**
   * Queue an inbound UTF-8 JSON frame for ordered protocol processing.
   *
   * Concurrent calls are processed in invocation order.
   */
  receive(message: string | Uint8Array): Promise<void>;
  /** Tell the protocol session that the host transport has already ended. */
  transportClosed(event?: OpenPrinterTransportClosedEvent): Promise<void>;
  /** Send one validated application-level server message. */
  send(message: OpenPrinterApplicationMessage): Promise<DeliveryResult>;
  /** Create and immediately hand off a canonical print-job message. */
  sendJob(job: PrintJob): Promise<DeliveryResult>;
  /** Ask this agent to report a complete printer inventory. */
  requestPrinters(): Promise<DeliveryResult>;
  /** Ask this agent to cancel a job not yet submitted. */
  cancelJob(cancellation: JobCancellation): Promise<DeliveryResult>;
  /** Notify this agent that host-owned configuration changed. */
  invalidateConfiguration(invalidation: ConfigurationInvalidation): Promise<DeliveryResult>;
  /** Send a semantic disconnect and ask the host to close its transport. */
  disconnect(options?: DisconnectAgentOptions): Promise<boolean>;
}

/**
 * Configured OpenPrinter protocol session factory.
 *
 * It owns no HTTP server, WebSocket listener, authentication policy,
 * connection registry, cluster coordination, or broker.
 */
export interface OpenPrinterServer<Metadata> {
  /** Validated endpoint paths integrations should mount. */
  readonly paths: OpenPrinterServerPaths;
  /** Build the current discovery response. */
  discover(): Promise<OpenPrinterDiscoveryDocument>;
  /** Create a cryptographically random, short-lived, one-time pairing code. */
  createPairingCode(input?: CreatePairingCodeInput<Metadata>): Promise<CreatedPairingCode<Metadata>>;
  /** Validate and redeem one JSON pairing request. */
  pair(input: unknown, context?: PairingAttemptContext): Promise<OpenPrinterPairingResponse | OpenPrinterErrorEnvelope>;
  /** Revoke a registered public credential. */
  revokeCredential(agentId: string, keyId: string): Promise<void>;
  /** Open one independent protocol session over a host-owned transport. */
  accept(input: AcceptOpenPrinterSessionInput): OpenPrinterSession<Metadata>;
  /** Convenience bridge for a `ws`-compatible socket without owning its listener. */
  handleGatewayConnection(socket: OpenPrinterGatewaySocket): OpenPrinterSession<Metadata>;
}

/** HTTP and WebSocket paths exposed by the server SDK. */
export interface OpenPrinterServerPaths {
  readonly discovery: string;
  readonly pairing: string;
  readonly gateway: string;
}
