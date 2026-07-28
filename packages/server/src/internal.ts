import type { Duplex } from 'node:stream';

import type { RawData } from 'ws';

import { OpenPrinterServerConfigurationError } from './errors.js';
import type { AuthenticationFailureReason, OpenPrinterServerOptions } from './types.js';

const BEARER_PATTERN = /^Bearer ([A-Za-z0-9\-._~+/]+={0,})$/i;
const IDENTIFIER_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;

export interface ParsedBearerToken {
  readonly ok: true;
  readonly token: string;
}

export interface InvalidBearerToken {
  readonly ok: false;
  readonly reason: Extract<AuthenticationFailureReason, 'missing-bearer-token' | 'malformed-bearer-token'>;
}

export function parseBearerToken(
  header: string | readonly string[] | undefined,
): ParsedBearerToken | InvalidBearerToken {
  if (header === undefined) {
    return { ok: false, reason: 'missing-bearer-token' };
  }

  if (typeof header !== 'string') {
    return { ok: false, reason: 'malformed-bearer-token' };
  }

  const match = BEARER_PATTERN.exec(header);
  if (match?.[1] === undefined) {
    return { ok: false, reason: 'malformed-bearer-token' };
  }

  return { ok: true, token: match[1] };
}

export function rawDataToBytes(data: RawData): Uint8Array {
  if (Array.isArray(data)) {
    return Buffer.concat(data);
  }

  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }

  return data;
}

export function rejectUpgrade(socket: Duplex, status: number, reason: string): void {
  if (socket.destroyed) {
    return;
  }

  const safeReason = reason.replaceAll(/[\r\n]/g, ' ');

  try {
    socket.end(
      `HTTP/1.1 ${status} ${safeReason}\r\n` +
        'Connection: close\r\n' +
        'Content-Length: 0\r\n' +
        'Cache-Control: no-store\r\n' +
        '\r\n',
    );
  } catch {
    socket.destroy();
  }
}

export function isValidIdentifier(value: string): boolean {
  return IDENTIFIER_PATTERN.test(value);
}

export function boundedCloseDetail(value: string): string | undefined {
  let sanitized = '';
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    sanitized += codePoint <= 0x1f || codePoint === 0x7f ? ' ' : character;
  }
  const normalized = sanitized.trim();

  return normalized === '' ? undefined : normalized.slice(0, 120);
}

export function callbackErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unknown callback error';
}

export function validateOptions<Metadata>(
  options: OpenPrinterServerOptions<Metadata>,
  protocolMaximumMessageBytes: number,
): {
  readonly authenticationTimeoutMs: number;
  readonly callbackTimeoutMs: number;
  readonly handshakeTimeoutMs: number;
  readonly heartbeatIntervalMs: number;
  readonly heartbeatTimeoutMs: number;
  readonly maxMessageBytes: number;
  readonly path?: string;
  readonly serverId: string;
  readonly serverVersion: string;
} {
  const authenticationTimeoutMs = options.authenticationTimeoutMs ?? 10_000;
  const callbackTimeoutMs = options.callbackTimeoutMs ?? 5_000;
  const handshakeTimeoutMs = options.handshakeTimeoutMs ?? 10_000;
  const heartbeatIntervalMs = options.heartbeatIntervalMs ?? 15_000;
  const heartbeatTimeoutMs = options.heartbeatTimeoutMs ?? 45_000;
  const maxMessageBytes = options.maxMessageBytes ?? protocolMaximumMessageBytes;
  const serverId = options.serverId ?? 'openprinter-server';
  const serverVersion = options.serverVersion ?? '0.1.0';

  requireIntegerInRange('authenticationTimeoutMs', authenticationTimeoutMs, 1, 300_000);
  requireIntegerInRange('callbackTimeoutMs', callbackTimeoutMs, 1, 30_000);
  requireIntegerInRange('handshakeTimeoutMs', handshakeTimeoutMs, 1, 300_000);
  requireIntegerInRange('heartbeatIntervalMs', heartbeatIntervalMs, 5_000, 300_000);
  requireIntegerInRange('heartbeatTimeoutMs', heartbeatTimeoutMs, 1_000, 120_000);
  requireIntegerInRange('maxMessageBytes', maxMessageBytes, 1_024, protocolMaximumMessageBytes);

  if (heartbeatTimeoutMs <= heartbeatIntervalMs) {
    throw new OpenPrinterServerConfigurationError('heartbeatTimeoutMs must be greater than heartbeatIntervalMs.');
  }
  if (callbackTimeoutMs >= heartbeatTimeoutMs) {
    throw new OpenPrinterServerConfigurationError('callbackTimeoutMs must be less than heartbeatTimeoutMs.');
  }

  if (!isValidIdentifier(serverId)) {
    throw new OpenPrinterServerConfigurationError('serverId must be a valid OpenPrinter identifier.');
  }

  if (serverVersion.length < 1 || serverVersion.length > 256) {
    throw new OpenPrinterServerConfigurationError('serverVersion must contain between 1 and 256 characters.');
  }

  if (
    options.path !== undefined &&
    (!options.path.startsWith('/') || options.path.includes('?') || options.path.includes('#'))
  ) {
    throw new OpenPrinterServerConfigurationError('path must be an absolute URL pathname without a query or fragment.');
  }

  return {
    authenticationTimeoutMs,
    callbackTimeoutMs,
    handshakeTimeoutMs,
    heartbeatIntervalMs,
    heartbeatTimeoutMs,
    maxMessageBytes,
    ...(options.path === undefined ? {} : { path: options.path }),
    serverId,
    serverVersion,
  };
}

function requireIntegerInRange(name: string, value: number, minimum: number, maximum: number): void {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new OpenPrinterServerConfigurationError(`${name} must be an integer between ${minimum} and ${maximum}.`);
  }
}
