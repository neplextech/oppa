import { Type, type Static, type TSchema } from '@sinclair/typebox';

import {
  MAX_PRINTERS_PER_INVENTORY,
  MAX_WIRE_MESSAGE_BYTES,
  PROTOCOL_SCHEMA_ID,
  PROTOCOL_VERSION,
} from '../constants.js';
import { IdentifierSchema, ProtocolVersionSchema, ShortStringSchema, TimestampSchema } from './common.js';
import { PrintJobSchema } from './job.js';
import { PrinterDescriptorSchema } from './printer.js';

function messageSchema<TType extends string, TPayload extends TSchema>(type: TType, payload: TPayload) {
  return Type.Object(
    {
      protocolVersion: ProtocolVersionSchema,
      messageId: IdentifierSchema,
      sentAt: TimestampSchema,
      type: Type.Literal(type),
      payload,
    },
    { additionalProperties: false },
  );
}

function correlatedMessageSchema<TType extends string, TPayload extends TSchema>(type: TType, payload: TPayload) {
  return Type.Object(
    {
      protocolVersion: ProtocolVersionSchema,
      messageId: IdentifierSchema,
      sentAt: TimestampSchema,
      correlationId: IdentifierSchema,
      type: Type.Literal(type),
      payload,
    },
    { additionalProperties: false },
  );
}

function optionallyCorrelatedMessageSchema<TType extends string, TPayload extends TSchema>(
  type: TType,
  payload: TPayload,
) {
  return Type.Object(
    {
      protocolVersion: ProtocolVersionSchema,
      messageId: IdentifierSchema,
      sentAt: TimestampSchema,
      correlationId: Type.Optional(IdentifierSchema),
      type: Type.Literal(type),
      payload,
    },
    { additionalProperties: false },
  );
}

/** Initial agent handshake and version advertisement. */
export const AgentHelloMessageSchema = messageSchema(
  'agent.hello',
  Type.Object(
    {
      agentId: IdentifierSchema,
      agentVersion: ShortStringSchema,
      productId: IdentifierSchema,
      productVersion: ShortStringSchema,
      supportedProtocolVersions: Type.Array(ProtocolVersionSchema, {
        minItems: 1,
        maxItems: 8,
        uniqueItems: true,
      }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of an agent hello envelope. */
export type AgentHelloMessage = Static<typeof AgentHelloMessageSchema>;

/** Agent hello payload without its wire envelope. */
export type AgentHello = AgentHelloMessage['payload'];

/** Correlated response to a server heartbeat request. */
export const AgentHeartbeatMessageSchema = correlatedMessageSchema(
  'agent.heartbeat',
  Type.Object(
    {
      uptimeSeconds: Type.Integer({
        minimum: 0,
        maximum: Number.MAX_SAFE_INTEGER,
      }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a correlated agent heartbeat. */
export type AgentHeartbeatMessage = Static<typeof AgentHeartbeatMessageSchema>;

/** Agent heartbeat payload without its wire envelope. */
export type AgentHeartbeat = AgentHeartbeatMessage['payload'];

/** Complete printer inventory, either periodic or request-correlated. */
export const PrinterInventoryMessageSchema = optionallyCorrelatedMessageSchema(
  'agent.printer_inventory',
  Type.Object(
    {
      revision: Type.Integer({
        minimum: 0,
        maximum: Number.MAX_SAFE_INTEGER,
      }),
      printers: Type.Array(PrinterDescriptorSchema, {
        maxItems: MAX_PRINTERS_PER_INVENTORY,
      }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a complete printer inventory. */
export type PrinterInventoryMessage = Static<typeof PrinterInventoryMessageSchema>;

/** Complete printer inventory payload. */
export type PrinterInventory = PrinterInventoryMessage['payload'];

/** Incremental printer inventory update. */
export const PrinterInventoryChangedMessageSchema = messageSchema(
  'agent.printer_inventory_changed',
  Type.Object(
    {
      revision: Type.Integer({
        minimum: 0,
        maximum: Number.MAX_SAFE_INTEGER,
      }),
      added: Type.Array(PrinterDescriptorSchema, {
        maxItems: MAX_PRINTERS_PER_INVENTORY,
      }),
      updated: Type.Array(PrinterDescriptorSchema, {
        maxItems: MAX_PRINTERS_PER_INVENTORY,
      }),
      removedPrinterIds: Type.Array(IdentifierSchema, {
        maxItems: MAX_PRINTERS_PER_INVENTORY,
        uniqueItems: true,
      }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of an incremental inventory update. */
export type PrinterInventoryChangedMessage = Static<typeof PrinterInventoryChangedMessageSchema>;

/** Incremental printer inventory payload. */
export type PrinterInventoryChanged = PrinterInventoryChangedMessage['payload'];

/** Durable-persistence acknowledgement for a delivered job. */
export const JobReceivedMessageSchema = correlatedMessageSchema(
  'agent.job_received',
  Type.Object(
    {
      jobId: IdentifierSchema,
      idempotencyKey: Type.String({ minLength: 1, maxLength: 256 }),
      status: Type.Literal('received'),
      receivedAt: TimestampSchema,
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a durable job acknowledgement. */
export type JobReceivedMessage = Static<typeof JobReceivedMessageSchema>;

/** Durable job acknowledgement payload. */
export type JobReceived = JobReceivedMessage['payload'];

/**
 * Submission result for a job accepted by the operating system or printer.
 *
 * This status does not claim that paper was physically produced.
 */
export const JobSubmittedMessageSchema = correlatedMessageSchema(
  'agent.job_submitted',
  Type.Object(
    {
      jobId: IdentifierSchema,
      idempotencyKey: Type.String({ minLength: 1, maxLength: 256 }),
      printerId: IdentifierSchema,
      status: Type.Literal('submitted'),
      submittedAt: TimestampSchema,
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a backend submission result. */
export type JobSubmittedMessage = Static<typeof JobSubmittedMessageSchema>;

/** Backend submission payload without its wire envelope. */
export type JobSubmitted = JobSubmittedMessage['payload'];

/** Recoverable or terminal failure after job delivery. */
export const JobFailedMessageSchema = correlatedMessageSchema(
  'agent.job_failed',
  Type.Object(
    {
      jobId: IdentifierSchema,
      idempotencyKey: Type.String({ minLength: 1, maxLength: 256 }),
      status: Type.Literal('failed'),
      failedAt: TimestampSchema,
      error: Type.Object(
        {
          code: IdentifierSchema,
          message: Type.String({ minLength: 1, maxLength: 1_024 }),
          retryable: Type.Boolean(),
        },
        { additionalProperties: false },
      ),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a job failure result. */
export type JobFailedMessage = Static<typeof JobFailedMessageSchema>;

/** Job failure payload without its wire envelope. */
export type JobFailed = JobFailedMessage['payload'];

/** Bounded, sanitized operational summary safe to send to the server. */
export const AgentDiagnosticsMessageSchema = messageSchema(
  'agent.diagnostics',
  Type.Object(
    {
      agentId: IdentifierSchema,
      collectedAt: TimestampSchema,
      health: Type.Union([Type.Literal('healthy'), Type.Literal('degraded'), Type.Literal('unhealthy')]),
      queueDepth: Type.Integer({
        minimum: 0,
        maximum: Number.MAX_SAFE_INTEGER,
      }),
      printersOnline: Type.Integer({
        minimum: 0,
        maximum: MAX_PRINTERS_PER_INVENTORY,
      }),
      printersTotal: Type.Integer({
        minimum: 0,
        maximum: MAX_PRINTERS_PER_INVENTORY,
      }),
      issues: Type.Array(
        Type.Object(
          {
            code: IdentifierSchema,
            message: Type.String({ minLength: 1, maxLength: 1_024 }),
            severity: Type.Union([Type.Literal('info'), Type.Literal('warning'), Type.Literal('error')]),
          },
          { additionalProperties: false },
        ),
        { maxItems: 64 },
      ),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of sanitized agent diagnostics. */
export type AgentDiagnosticsMessage = Static<typeof AgentDiagnosticsMessageSchema>;

/** Sanitized agent diagnostics payload. */
export type AgentDiagnostics = AgentDiagnosticsMessage['payload'];

/** Any message an OPPA agent may send to a server. */
export const AgentMessageSchema = Type.Union(
  [
    AgentHelloMessageSchema,
    AgentHeartbeatMessageSchema,
    PrinterInventoryMessageSchema,
    PrinterInventoryChangedMessageSchema,
    JobReceivedMessageSchema,
    JobSubmittedMessageSchema,
    JobFailedMessageSchema,
    AgentDiagnosticsMessageSchema,
  ],
  { title: 'AgentMessage' },
);

/** TypeScript representation of any validated agent-to-server message. */
export type AgentMessage = Static<typeof AgentMessageSchema>;

const BRAND_UNSAFE_CHARACTER_CLASS =
  '\\u0000-\\u001F\\u007F-\\u009F\\u061C\\u200E\\u200F\\u202A-\\u202E\\u2066-\\u2069';
const BRAND_EDGE_WHITESPACE_CHARACTER_CLASS =
  '\\u0020\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000\\uFEFF';
const BRAND_SURROGATE_CHARACTER_CLASS = '\\uD800-\\uDFFF';
const BRAND_UNICODE_SCALAR_PATTERN =
  `(?:[\\uD800-\\uDBFF][\\uDC00-\\uDFFF]|` + `[^${BRAND_UNSAFE_CHARACTER_CLASS}${BRAND_SURROGATE_CHARACTER_CLASS}])`;
const BRAND_SAFE_EDGE_PATTERN =
  `(?:[\\uD800-\\uDBFF][\\uDC00-\\uDFFF]|` +
  `[^${BRAND_UNSAFE_CHARACTER_CLASS}${BRAND_EDGE_WHITESPACE_CHARACTER_CLASS}${BRAND_SURROGATE_CHARACTER_CLASS}])`;
const BRAND_NAME_PATTERN = `^${BRAND_SAFE_EDGE_PATTERN}(?:${BRAND_UNICODE_SCALAR_PATTERN}*${BRAND_SAFE_EDGE_PATTERN})?$`;

/** A bounded service name safe to render without Unicode direction spoofing. */
export const OpenPrinterBrandNameSchema = Type.String({
  minLength: 1,
  maxLength: 256,
  pattern: BRAND_NAME_PATTERN,
});

/**
 * Human-readable identity of the service that accepted an agent connection.
 *
 * External icon and image URLs are intentionally excluded so an agent never
 * needs to load a third-party resource merely to present connection identity.
 */
export const OpenPrinterBrandMetadataSchema = Type.Object(
  {
    name: OpenPrinterBrandNameSchema,
  },
  {
    additionalProperties: false,
    title: 'OpenPrinterBrandMetadata',
  },
);

/** Safe, display-only service identity advertised during the handshake. */
export type OpenPrinterBrandMetadata = Static<typeof OpenPrinterBrandMetadataSchema>;

/** Server handshake response and selected protocol version. */
export const ServerHelloMessageSchema = correlatedMessageSchema(
  'server.hello',
  Type.Object(
    {
      serverId: IdentifierSchema,
      serverVersion: ShortStringSchema,
      brand: OpenPrinterBrandMetadataSchema,
      sessionId: IdentifierSchema,
      supportedProtocolVersions: Type.Array(ProtocolVersionSchema, {
        minItems: 1,
        maxItems: 8,
        uniqueItems: true,
      }),
      selectedProtocolVersion: ProtocolVersionSchema,
      heartbeatIntervalMs: Type.Integer({
        minimum: 5_000,
        maximum: 300_000,
      }),
      maxMessageBytes: Type.Integer({
        minimum: 1_024,
        maximum: MAX_WIRE_MESSAGE_BYTES,
      }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a server hello response. */
export type ServerHelloMessage = Static<typeof ServerHelloMessageSchema>;

/** Server hello payload without its wire envelope. */
export type ServerHello = ServerHelloMessage['payload'];

/** Server liveness probe; the agent responds using its message ID. */
export const HeartbeatRequestMessageSchema = messageSchema(
  'server.heartbeat',
  Type.Object(
    {
      timeoutMs: Type.Integer({ minimum: 1_000, maximum: 120_000 }),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a server heartbeat request. */
export type HeartbeatRequestMessage = Static<typeof HeartbeatRequestMessageSchema>;

/** Server heartbeat request payload. */
export type HeartbeatRequest = HeartbeatRequestMessage['payload'];

/** At-least-once delivery of a concrete printer job. */
export const PrintJobMessageSchema = messageSchema('server.print_job', PrintJobSchema);

/** TypeScript representation of an at-least-once job delivery. */
export type PrintJobMessage = Static<typeof PrintJobMessageSchema>;

/** Request cancellation of a job that has not yet been submitted. */
export const CancelJobMessageSchema = messageSchema(
  'server.cancel_job',
  Type.Object(
    {
      jobId: IdentifierSchema,
      reason: Type.Optional(Type.String({ minLength: 1, maxLength: 512 })),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of a job cancellation request. */
export type CancelJobMessage = Static<typeof CancelJobMessageSchema>;

/** Job cancellation payload without its wire envelope. */
export type CancelJob = CancelJobMessage['payload'];

/** Request a full inventory snapshot. */
export const RequestPrinterInventoryMessageSchema = messageSchema(
  'server.request_printer_inventory',
  Type.Object({}, { additionalProperties: false }),
);

/** TypeScript representation of a full inventory request. */
export type RequestPrinterInventoryMessage = Static<typeof RequestPrinterInventoryMessageSchema>;

/** Empty full-inventory request payload. */
export type RequestPrinterInventory = RequestPrinterInventoryMessage['payload'];

/** Notify an agent that host-owned configuration should be refreshed. */
export const ConfigurationInvalidatedMessageSchema = messageSchema(
  'server.configuration_invalidated',
  Type.Object(
    {
      scope: Type.Union([Type.Literal('agent'), Type.Literal('printers'), Type.Literal('all')]),
      revision: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of configuration invalidation. */
export type ConfigurationInvalidatedMessage = Static<typeof ConfigurationInvalidatedMessageSchema>;

/** Configuration invalidation payload. */
export type ConfigurationInvalidated = ConfigurationInvalidatedMessage['payload'];

/** Explain an intentional server disconnect and reconnection policy. */
export const DisconnectMessageSchema = messageSchema(
  'server.disconnect',
  Type.Object(
    {
      code: IdentifierSchema,
      reason: Type.String({ minLength: 1, maxLength: 1_024 }),
      reconnect: Type.Boolean(),
      retryAfterMs: Type.Optional(Type.Integer({ minimum: 0, maximum: 86_400_000 })),
    },
    { additionalProperties: false },
  ),
);

/** TypeScript representation of an intentional disconnect. */
export type DisconnectMessage = Static<typeof DisconnectMessageSchema>;

/** Intentional disconnect payload without its wire envelope. */
export type Disconnect = DisconnectMessage['payload'];

/** Any message an OpenPrinter server may send to an agent. */
export const ServerMessageSchema = Type.Union(
  [
    ServerHelloMessageSchema,
    HeartbeatRequestMessageSchema,
    PrintJobMessageSchema,
    CancelJobMessageSchema,
    RequestPrinterInventoryMessageSchema,
    ConfigurationInvalidatedMessageSchema,
    DisconnectMessageSchema,
  ],
  { title: 'ServerMessage' },
);

/** TypeScript representation of any validated server-to-agent message. */
export type ServerMessage = Static<typeof ServerMessageSchema>;

/** The complete TypeBox source of truth for OpenPrinter protocol v1. */
export const OpenPrinterProtocolSchema = Type.Union([AgentMessageSchema, ServerMessageSchema], {
  $schema: 'https://json-schema.org/draft/2020-12/schema',
  $id: PROTOCOL_SCHEMA_ID,
  title: 'OpenPrinterProtocolMessage',
  description: 'A version 1 OpenPrinter message. Submission does not imply verified physical printing.',
});

/** TypeScript representation of any OpenPrinter v1 wire message. */
export type OpenPrinterProtocolMessage = Static<typeof OpenPrinterProtocolSchema>;

/** Stable discriminator strings accepted from OPPA agents. */
export const AGENT_MESSAGE_TYPES = [
  'agent.hello',
  'agent.heartbeat',
  'agent.printer_inventory',
  'agent.printer_inventory_changed',
  'agent.job_received',
  'agent.job_submitted',
  'agent.job_failed',
  'agent.diagnostics',
] as const;

/** Stable discriminator strings accepted from OpenPrinter servers. */
export const SERVER_MESSAGE_TYPES = [
  'server.hello',
  'server.heartbeat',
  'server.print_job',
  'server.cancel_job',
  'server.request_printer_inventory',
  'server.configuration_invalidated',
  'server.disconnect',
] as const;

/** The version advertised in hello fixtures and generated schemas. */
export const CURRENT_PROTOCOL_VERSION = PROTOCOL_VERSION;
