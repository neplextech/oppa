import type { TSchema } from '@sinclair/typebox';
import { Value } from '@sinclair/typebox/value';

import { MAX_AUTH_MESSAGE_BYTES, MAX_WIRE_MESSAGE_BYTES, PROTOCOL_VERSION } from './constants.js';
import {
  ProtocolError,
  ProtocolValidationError,
  UnsupportedProtocolVersionError,
  type ProtocolIssue,
} from './errors.js';
import {
  GatewayAuthenticationResponseSchema,
  GatewayAuthenticationServerMessageSchema,
  OpenPrinterDiscoveryDocumentSchema,
  OpenPrinterErrorEnvelopeSchema,
  OpenPrinterPairingRequestSchema,
  OpenPrinterPairingResponseSchema,
  PublicEd25519JwkSchema,
  type GatewayAuthenticationResponse,
  type GatewayAuthenticationServerMessage,
  type OpenPrinterDiscoveryDocument,
  type OpenPrinterErrorEnvelope,
  type OpenPrinterPairingRequest,
  type OpenPrinterPairingResponse,
  type PublicEd25519Jwk,
} from './schemas/auth.js';
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

function decodeBoundedJson(input: string | Uint8Array, maximumBytes: number): unknown {
  const size = typeof input === 'string' ? byteLength(input) : input.byteLength;
  if (size > maximumBytes) {
    throw new ProtocolError('message_too_large', `Protocol message exceeds the ${maximumBytes}-byte limit`);
  }
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

/** Validates a discovery document and rejects unsupported protocol versions. */
export function parseDiscoveryDocument(value: unknown): OpenPrinterDiscoveryDocument {
  return parseWithSchema<OpenPrinterDiscoveryDocument>(
    OpenPrinterDiscoveryDocumentSchema,
    value,
    'OpenPrinter discovery document',
    true,
  );
}

/** Validates a public Ed25519 JWK including its 32-byte public key. */
export function parsePublicEd25519Jwk(value: unknown): PublicEd25519Jwk {
  const key = parseWithSchema<PublicEd25519Jwk>(PublicEd25519JwkSchema, value, 'public Ed25519 JWK');
  if (decodeBase64Url(key.x).byteLength !== 32) {
    throw new ProtocolValidationError('Invalid public Ed25519 JWK', [
      { path: '/x', message: 'must decode to exactly 32 bytes' },
    ]);
  }
  return key;
}

/** Validates a pairing request and its public key. */
export function parsePairingRequest(value: unknown): OpenPrinterPairingRequest {
  const request = parseWithSchema<OpenPrinterPairingRequest>(
    OpenPrinterPairingRequestSchema,
    value,
    'OpenPrinter pairing request',
    true,
  );
  parsePublicEd25519Jwk(request.credential.publicKey);
  return request;
}

/** Validates a successful pairing response. */
export function parsePairingResponse(value: unknown): OpenPrinterPairingResponse {
  return parseWithSchema<OpenPrinterPairingResponse>(
    OpenPrinterPairingResponseSchema,
    value,
    'OpenPrinter pairing response',
  );
}

/** Validates a machine-readable pairing error response. */
export function parseOpenPrinterError(value: unknown): OpenPrinterErrorEnvelope {
  return parseWithSchema<OpenPrinterErrorEnvelope>(OpenPrinterErrorEnvelopeSchema, value, 'OpenPrinter error');
}

/** Validates a client gateway authentication response. */
export function parseGatewayAuthenticationResponse(value: unknown): GatewayAuthenticationResponse {
  return parseWithSchema<GatewayAuthenticationResponse>(
    GatewayAuthenticationResponseSchema,
    value,
    'gateway authentication response',
  );
}

/** Validates a server gateway authentication message. */
export function parseGatewayAuthenticationServerMessage(value: unknown): GatewayAuthenticationServerMessage {
  return parseWithSchema<GatewayAuthenticationServerMessage>(
    GatewayAuthenticationServerMessageSchema,
    value,
    'gateway authentication message',
  );
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

/** Decodes and validates one bounded client authentication frame. */
export function decodeGatewayAuthenticationResponse(input: string | Uint8Array): GatewayAuthenticationResponse {
  return parseGatewayAuthenticationResponse(decodeBoundedJson(input, MAX_AUTH_MESSAGE_BYTES));
}

/** Decodes and validates one bounded server authentication frame. */
export function decodeGatewayAuthenticationServerMessage(
  input: string | Uint8Array,
): GatewayAuthenticationServerMessage {
  return parseGatewayAuthenticationServerMessage(decodeBoundedJson(input, MAX_AUTH_MESSAGE_BYTES));
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

/** Validates and encodes a client authentication response. */
export function encodeGatewayAuthenticationResponse(message: GatewayAuthenticationResponse): string {
  const encoded = JSON.stringify(parseGatewayAuthenticationResponse(message));
  if (byteLength(encoded) > MAX_AUTH_MESSAGE_BYTES) {
    throw new ProtocolError('message_too_large', 'Gateway authentication response exceeds the size limit');
  }
  return encoded;
}

/** Validates and encodes a server authentication message. */
export function encodeGatewayAuthenticationServerMessage(message: GatewayAuthenticationServerMessage): string {
  const encoded = JSON.stringify(parseGatewayAuthenticationServerMessage(message));
  if (byteLength(encoded) > MAX_AUTH_MESSAGE_BYTES) {
    throw new ProtocolError('message_too_large', 'Gateway authentication message exceeds the size limit');
  }
  return encoded;
}

/** Returns whether a string is canonical unpadded RFC 4648 base64url. */
export function isUnpaddedBase64Url(value: string): boolean {
  if (value.length === 0 || !/^[A-Za-z0-9_-]+$/.test(value) || value.length % 4 === 1) {
    return false;
  }
  try {
    return encodeBase64Url(decodeBase64Url(value)) === value;
  } catch {
    return false;
  }
}

/** Decodes canonical unpadded RFC 4648 base64url without relying on Node APIs. */
export function decodeBase64Url(value: string): Uint8Array {
  if (value.length === 0 || !/^[A-Za-z0-9_-]+$/.test(value) || value.length % 4 === 1) {
    throw new ProtocolValidationError('Invalid base64url value', [
      { path: '/', message: 'must be canonical unpadded base64url' },
    ]);
  }
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  const output = new Uint8Array(Math.floor((value.length * 6) / 8));
  let accumulator = 0;
  let bits = 0;
  let index = 0;
  for (const character of value) {
    accumulator = (accumulator << 6) | alphabet.indexOf(character);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      output[index++] = (accumulator >>> bits) & 0xff;
      accumulator &= bits === 0 ? 0 : (1 << bits) - 1;
    }
  }
  if (bits > 0 && (accumulator & ((1 << bits) - 1)) !== 0) {
    throw new ProtocolValidationError('Invalid base64url value', [
      { path: '/', message: 'contains non-canonical trailing bits' },
    ]);
  }
  return output;
}

/** Encodes bytes as canonical unpadded RFC 4648 base64url. */
export function encodeBase64Url(value: Uint8Array): string {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  let output = '';
  let accumulator = 0;
  let bits = 0;
  for (const byte of value) {
    accumulator = (accumulator << 8) | byte;
    bits += 8;
    while (bits >= 6) {
      bits -= 6;
      output += alphabet[(accumulator >>> bits) & 0x3f];
      accumulator &= bits === 0 ? 0 : (1 << bits) - 1;
    }
  }
  if (bits > 0) {
    output += alphabet[(accumulator << (6 - bits)) & 0x3f];
  }
  return output;
}
