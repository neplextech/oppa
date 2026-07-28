import type { TSchema } from '@sinclair/typebox';
import { Value } from '@sinclair/typebox/value';

import { MAX_WIRE_MESSAGE_BYTES, PROTOCOL_VERSION } from './constants.js';
import {
  ProtocolError,
  ProtocolValidationError,
  UnsupportedProtocolVersionError,
  type ProtocolIssue,
} from './errors.js';
import { PrintDocumentSchema, type PrintDocument } from './schemas/document.js';
import { PrintJobSchema, type PrintJob } from './schemas/job.js';
import {
  AgentMessageSchema,
  OpenPrinterProtocolSchema,
  ServerMessageSchema,
  type AgentMessage,
  type OpenPrinterProtocolMessage,
  type ServerMessage,
} from './schemas/messages.js';
import { PrinterDescriptorSchema, type PrinterDescriptor } from './schemas/printer.js';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

function normalizedIssues(schema: TSchema, value: unknown): ProtocolIssue[] {
  const issues: ProtocolIssue[] = [];
  for (const error of Value.Errors(schema, value)) {
    issues.push({
      path: error.path || '/',
      message: error.message,
    });
    if (issues.length === 16) {
      break;
    }
  }
  return issues;
}

function assertSupportedVersion(value: unknown): void {
  if (typeof value !== 'object' || value === null || !Object.prototype.hasOwnProperty.call(value, 'protocolVersion')) {
    return;
  }

  const version = (value as { protocolVersion?: unknown }).protocolVersion;
  if (
    version !== PROTOCOL_VERSION &&
    (version === null || typeof version === 'string' || typeof version === 'number' || typeof version === 'boolean')
  ) {
    throw new UnsupportedProtocolVersionError(version);
  }
}

function parseWithSchema<T>(schema: TSchema, value: unknown, description: string, checkVersion = false): T {
  if (checkVersion) {
    assertSupportedVersion(value);
  }
  if (!Value.Check(schema, value)) {
    throw new ProtocolValidationError(`Invalid ${description}`, normalizedIssues(schema, value));
  }
  return value as T;
}

function byteLength(value: string): number {
  return encoder.encode(value).byteLength;
}

function enforceWireSize(size: number): void {
  if (size > MAX_WIRE_MESSAGE_BYTES) {
    throw new ProtocolError(
      'message_too_large',
      `Protocol message is ${size} bytes; limit is ${MAX_WIRE_MESSAGE_BYTES} bytes`,
    );
  }
}

function decodeJson(input: string | Uint8Array): unknown {
  if (typeof input === 'string' && input.length > MAX_WIRE_MESSAGE_BYTES) {
    throw new ProtocolError('message_too_large', `Protocol message exceeds the ${MAX_WIRE_MESSAGE_BYTES}-byte limit`);
  }
  enforceWireSize(typeof input === 'string' ? byteLength(input) : input.byteLength);

  let text: string;
  try {
    text = typeof input === 'string' ? input : decoder.decode(input);
  } catch {
    throw new ProtocolError('invalid_json', 'Protocol message is not valid UTF-8');
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    throw new ProtocolError('invalid_json', 'Protocol message is not valid JSON');
  }
}

function encodeJson<T>(message: T, validate: (value: unknown) => T): string {
  validate(message);
  const encoded = JSON.stringify(message);
  enforceWireSize(byteLength(encoded));
  return encoded;
}

/** Validates an unknown value as an agent-to-server message. */
export function parseAgentMessage(value: unknown): AgentMessage {
  return parseWithSchema<AgentMessage>(AgentMessageSchema, value, 'agent message', true);
}

/** Validates an unknown value as a server-to-agent message. */
export function parseServerMessage(value: unknown): ServerMessage {
  return parseWithSchema<ServerMessage>(ServerMessageSchema, value, 'server message', true);
}

/** Validates an unknown value as either OpenPrinter message direction. */
export function parseProtocolMessage(value: unknown): OpenPrinterProtocolMessage {
  return parseWithSchema<OpenPrinterProtocolMessage>(
    OpenPrinterProtocolSchema,
    value,
    'OpenPrinter protocol message',
    true,
  );
}

/** Validates an unknown value as a structured print document. */
export function parsePrintDocument(value: unknown): PrintDocument {
  return parseWithSchema<PrintDocument>(PrintDocumentSchema, value, 'print document');
}

/** Validates an unknown value as a concrete, idempotent print job. */
export function parsePrintJob(value: unknown): PrintJob {
  return parseWithSchema<PrintJob>(PrintJobSchema, value, 'print job');
}

/** Validates an unknown value as a printer descriptor. */
export function parsePrinterDescriptor(value: unknown): PrinterDescriptor {
  return parseWithSchema<PrinterDescriptor>(PrinterDescriptorSchema, value, 'printer descriptor');
}

/** Decodes UTF-8 JSON and validates an agent-to-server message. */
export function decodeAgentMessage(input: string | Uint8Array): AgentMessage {
  return parseAgentMessage(decodeJson(input));
}

/** Decodes UTF-8 JSON and validates a server-to-agent message. */
export function decodeServerMessage(input: string | Uint8Array): ServerMessage {
  return parseServerMessage(decodeJson(input));
}

/** Decodes UTF-8 JSON and validates either protocol message direction. */
export function decodeProtocolMessage(input: string | Uint8Array): OpenPrinterProtocolMessage {
  return parseProtocolMessage(decodeJson(input));
}

/** Validates and encodes an agent message as compact JSON. */
export function encodeAgentMessage(message: AgentMessage): string {
  return encodeJson(message, parseAgentMessage);
}

/** Validates and encodes a server message as compact JSON. */
export function encodeServerMessage(message: ServerMessage): string {
  return encodeJson(message, parseServerMessage);
}

/** Validates and encodes either direction as compact JSON. */
export function encodeProtocolMessage(message: OpenPrinterProtocolMessage): string {
  return encodeJson(message, parseProtocolMessage);
}
