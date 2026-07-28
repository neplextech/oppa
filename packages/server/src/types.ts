import type { IncomingMessage } from 'node:http';
import type { Duplex } from 'node:stream';

import type { AgentMessage, PrintJob, PrinterDescriptor, ServerMessage } from '@openprinter/protocol';

/** A value or promise accepted by lifecycle callbacks. */
export type Awaitable<Value> = Value | PromiseLike<Value>;

/** Select one canonical agent message using its wire discriminator. */
export type AgentMessageOf<Type extends AgentMessage['type']> = Extract<AgentMessage, { readonly type: Type }>;

/** Select one canonical server message using its wire discriminator. */
export type ServerMessageOf<Type extends ServerMessage['type']> = Extract<ServerMessage, { readonly type: Type }>;

/** Input passed to the host application's Bearer-token verifier. */
export interface AuthenticateAgentInput {
  /** The opaque Bearer token, which the SDK never interprets or logs. */
  readonly token: string;
  /** The original HTTP upgrade request. */
  readonly request: IncomingMessage;
  /** Aborted when authentication times out or the peer disconnects. */
  readonly signal: AbortSignal;
}

/** Authenticated identity returned by the host application. */
export interface AuthenticatedAgent<Metadata> {
  /** Stable identity authorized by the Bearer token. */
  readonly agentId: string;
  /** Host-owned, non-secret context retained with the live session. */
  readonly metadata?: Metadata;
}

/** Immutable public snapshot of an authenticated live agent session. */
export interface ConnectedAgent<Metadata> {
  /** Stable identity authorized by the host application. */
  readonly agentId: string;
  /** Unique identifier for this WebSocket session. */
  readonly sessionId: string;
  /** Host-owned metadata supplied by the authentication callback. */
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

/** Common context for a validated message from an authenticated agent. */
export interface AgentMessageEvent<Metadata, Message extends AgentMessage> {
  /** Snapshot of the agent at callback dispatch time. */
  readonly agent: ConnectedAgent<Metadata>;
  /** Canonical, runtime-validated protocol message. */
  readonly message: Message;
}

/** A completed agent handshake. */
export interface AgentConnectedEvent<Metadata> {
  /** Newly registered live agent. */
  readonly agent: ConnectedAgent<Metadata>;
}

/** Why a previously connected session left the registry. */
export type AgentDisconnectReason =
  | 'peer-closed'
  | 'connection-replaced'
  | 'heartbeat-timeout'
  | 'protocol-error'
  | 'server-disconnect'
  | 'server-shutdown'
  | 'transport-error';

/** A connected agent session leaving the live registry. */
export interface AgentDisconnectedEvent<Metadata> {
  /** Final snapshot of the disconnected agent. */
  readonly agent: ConnectedAgent<Metadata>;
  /** Stable lifecycle reason independent of WebSocket wording. */
  readonly reason: AgentDisconnectReason;
  /** WebSocket close code, when the peer supplied one. */
  readonly closeCode?: number;
  /** Bounded, non-sensitive close detail. */
  readonly detail?: string;
}

/** Authentication rejection categories exposed without the rejected token. */
export type AuthenticationFailureReason =
  | 'missing-bearer-token'
  | 'malformed-bearer-token'
  | 'rejected'
  | 'callback-error'
  | 'timeout'
  | 'invalid-agent';

/** A rejected HTTP upgrade, intentionally excluding authorization headers. */
export interface AuthenticationFailedEvent {
  /** Stable rejection category. */
  readonly reason: AuthenticationFailureReason;
  /** Remote socket address, when Node supplied one. */
  readonly remoteAddress?: string;
}

/** Inventory event emitted after the in-memory session view is updated. */
export interface PrintersChangedEvent<Metadata> {
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
  /** Timed-out agent. */
  readonly agent: ConnectedAgent<Metadata>;
  /** UTC time of its latest correlated heartbeat response. */
  readonly lastHeartbeatAt: string;
  /** Configured response timeout. */
  readonly timeoutMs: number;
}

/** Stable categories for rejected agent transport behavior. */
export type ServerProtocolErrorCode =
  | 'binary-message'
  | 'handshake-timeout'
  | 'identity-mismatch'
  | 'invalid-message'
  | 'message-too-large'
  | 'unexpected-message'
  | 'unsupported-protocol-version';

/** A payload-free protocol failure from one connection. */
export interface ServerProtocolErrorEvent<Metadata> {
  /** Authenticated identity, even when the hello was not completed. */
  readonly agentId: string;
  /** Connected snapshot when the handshake had completed. */
  readonly agent?: ConnectedAgent<Metadata>;
  /** Stable transport-level error category. */
  readonly code: ServerProtocolErrorCode;
  /** Error object without the rejected payload. */
  readonly error: Error;
}

/** Names of user callbacks isolated by the SDK. */
export type OpenPrinterCallbackName =
  | 'onAgentConnected'
  | 'onAgentDisconnected'
  | 'onAuthenticationFailed'
  | 'onAuthenticationMetadata'
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

/** Configurable SDK construction and lifecycle callbacks. */
export interface OpenPrinterServerOptions<Metadata> {
  /**
   * Verify an opaque Bearer token and return its authoritative agent identity.
   * Returning `null` rejects the upgrade.
   */
  readonly authenticateAgent: (input: AuthenticateAgentInput) => Awaitable<AuthenticatedAgent<Metadata> | null>;
  /** Optional URL pathname exclusively handled by this upgrade listener. */
  readonly path?: string;
  /** Stable server identifier advertised during the handshake. */
  readonly serverId?: string;
  /** Human-readable server SDK or host version advertised to agents. */
  readonly serverVersion?: string;
  /** Maximum accepted WebSocket payload in bytes. */
  readonly maxMessageBytes?: number;
  /** Time allowed for the first validated `agent.hello`. */
  readonly handshakeTimeoutMs?: number;
  /** Time allowed for the host authentication callback. */
  readonly authenticationTimeoutMs?: number;
  /** Maximum time allowed for any lifecycle callback before it is isolated. */
  readonly callbackTimeoutMs?: number;
  /** Frequency of server heartbeat requests. */
  readonly heartbeatIntervalMs?: number;
  /** Maximum time between correlated heartbeat responses. */
  readonly heartbeatTimeoutMs?: number;
  /** Called after a session is authenticated, negotiated, and registered. */
  readonly onAgentConnected?: (event: AgentConnectedEvent<Metadata>) => Awaitable<void>;
  /** Called exactly once for a session that had entered the live registry. */
  readonly onAgentDisconnected?: (event: AgentDisconnectedEvent<Metadata>) => Awaitable<void>;
  /** Called for safe, non-secret authentication rejection metadata. */
  readonly onAuthenticationFailed?: (event: AuthenticationFailedEvent) => Awaitable<void>;
  /** Called for optional, validated, non-secret agent authentication context. */
  readonly onAuthenticationMetadata?: (
    event: AgentMessageEvent<Metadata, AgentMessageOf<'agent.authentication_metadata'>>,
  ) => Awaitable<void>;
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
  /** Called before a heartbeat-expired session is removed. */
  readonly onHeartbeatTimeout?: (event: HeartbeatTimeoutEvent<Metadata>) => Awaitable<void>;
  /** Called when an inbound transport or protocol invariant is rejected. */
  readonly onProtocolError?: (event: ServerProtocolErrorEvent<Metadata>) => Awaitable<void>;
  /** Called when another user-provided callback throws or rejects. */
  readonly onCallbackError?: (event: CallbackErrorEvent) => Awaitable<void>;
}

/** Successful immediate handoff to a live WebSocket connection. */
export interface DeliverySuccess {
  /** Distinguishes successful delivery results. */
  readonly ok: true;
  /** Agent selected by the host application. */
  readonly agentId: string;
  /** Canonical wire message identifier used for correlation. */
  readonly messageId: string;
  /** UTC time encoded in the delivered protocol message. */
  readonly sentAt: string;
}

/** Reasons a message could not be handed to a live agent connection. */
export type DeliveryFailureReason = 'agent-offline' | 'connection-closed' | 'server-closed';

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

/** Immediate WebSocket handoff result; it is not a persistence acknowledgement. */
export type DeliveryResult = DeliverySuccess | DeliveryFailure;

/** Host-supplied reason for intentionally disconnecting one agent. */
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

/**
 * Framework-neutral OpenPrinter server attached to an existing HTTP server.
 */
export interface OpenPrinterServer<Metadata> {
  /**
   * HTTP `upgrade` listener. It is bound and may be passed directly to
   * `httpServer.on("upgrade", openPrinter.handleUpgrade)`.
   */
  readonly handleUpgrade: (request: IncomingMessage, socket: Duplex, head: Buffer) => void;
  /** Whether this SDK instance has stopped accepting and sending traffic. */
  readonly closed: boolean;
  /** Return snapshots of all authenticated and negotiated live agents. */
  listAgents(): readonly ConnectedAgent<Metadata>[];
  /** Return one live agent snapshot, or `null` when it is offline. */
  getAgent(agentId: string): ConnectedAgent<Metadata> | null;
  /** Return the latest complete in-memory inventory for one live agent. */
  getPrinters(agentId: string): readonly PrinterDescriptor[];
  /** Send an already formed, canonical server protocol message. */
  send(agentId: string, message: ServerMessage): Promise<DeliveryResult>;
  /** Create and immediately deliver a canonical print-job message. */
  sendJob(agentId: string, job: PrintJob): Promise<DeliveryResult>;
  /** Ask a live agent to report a complete printer inventory. */
  requestPrinters(agentId: string): Promise<DeliveryResult>;
  /** Ask a live agent to cancel a job not yet submitted. */
  cancelJob(agentId: string, cancellation: JobCancellation): Promise<DeliveryResult>;
  /** Notify a live agent that host-owned configuration changed. */
  invalidateConfiguration(agentId: string, invalidation: ConfigurationInvalidation): Promise<DeliveryResult>;
  /** Intentionally close one live session. */
  disconnect(agentId: string, options?: DisconnectAgentOptions): Promise<boolean>;
  /** Stop upgrades, discard live state, and close every agent connection. */
  close(): Promise<void>;
}
