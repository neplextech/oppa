import {
  MAX_WIRE_MESSAGE_BYTES,
  OPENPRINTER_DISCOVERY_PATH,
  OPENPRINTER_GATEWAY_PATH,
  OPENPRINTER_PAIRING_PATH,
  type OpenPrinterBrandMetadata,
} from '@openprinter/protocol';

import { OpenPrinterServerConfigurationError } from './errors.js';
import type {
  AcceptOpenPrinterSessionInput,
  OpenPrinterServerOptions,
  OpenPrinterServerPaths,
  OpenPrinterTransportCloseRequest,
} from './types.js';

const IDENTIFIER_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const encoder = new TextEncoder();

/** Validated server configuration shared by all accepted sessions. */
export interface ResolvedOpenPrinterServerOptions {
  readonly authenticationTimeoutMs: number;
  readonly brand: OpenPrinterBrandMetadata;
  readonly callbackTimeoutMs: number;
  readonly handshakeTimeoutMs: number;
  readonly heartbeatIntervalMs: number;
  readonly heartbeatTimeoutMs: number;
  readonly maxMessageBytes: number;
  readonly paths: OpenPrinterServerPaths;
  readonly serverId: string;
  readonly serverVersion: string;
  readonly transportTimeoutMs: number;
  readonly challengeTtlMs: number;
}

export function isValidIdentifier(value: string): boolean {
  return IDENTIFIER_PATTERN.test(value);
}

export function boundedDetail(value: string): string | undefined {
  let sanitized = '';
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    sanitized += codePoint <= 0x1f || codePoint === 0x7f ? ' ' : character;
  }
  const normalized = sanitized.trim();
  return normalized === '' ? undefined : normalized.slice(0, 120);
}

export function utf8ByteLength(value: string): number {
  return encoder.encode(value).byteLength;
}

export function validateOptions<Metadata>(
  options: OpenPrinterServerOptions<Metadata>,
): ResolvedOpenPrinterServerOptions {
  if (typeof options !== 'object' || options === null) {
    throw new OpenPrinterServerConfigurationError('Server options must be an object.');
  }

  const callbackTimeoutMs = options.callbackTimeoutMs ?? 5_000;
  const authenticationTimeoutMs = options.authenticationTimeoutMs ?? 30_000;
  const challengeTtlMs = options.challengeTtlMs ?? 30_000;
  const handshakeTimeoutMs = options.handshakeTimeoutMs ?? 10_000;
  const heartbeatIntervalMs = options.heartbeatIntervalMs ?? 15_000;
  const heartbeatTimeoutMs = options.heartbeatTimeoutMs ?? 45_000;
  const maxMessageBytes = options.maxMessageBytes ?? MAX_WIRE_MESSAGE_BYTES;
  const serverId = options.serverId ?? 'openprinter-server';
  const serverVersion = options.serverVersion ?? '0.1.0';
  const transportTimeoutMs = options.transportTimeoutMs ?? 10_000;

  requireIntegerInRange('callbackTimeoutMs', callbackTimeoutMs, 1, 30_000);
  requireIntegerInRange('authenticationTimeoutMs', authenticationTimeoutMs, 1_000, 300_000);
  requireIntegerInRange('challengeTtlMs', challengeTtlMs, 5_000, 300_000);
  requireIntegerInRange('handshakeTimeoutMs', handshakeTimeoutMs, 1, 300_000);
  requireIntegerInRange('heartbeatIntervalMs', heartbeatIntervalMs, 5_000, 300_000);
  requireIntegerInRange('heartbeatTimeoutMs', heartbeatTimeoutMs, 1_000, 120_000);
  requireIntegerInRange('maxMessageBytes', maxMessageBytes, 1_024, MAX_WIRE_MESSAGE_BYTES);
  requireIntegerInRange('transportTimeoutMs', transportTimeoutMs, 1, 120_000);

  if (heartbeatTimeoutMs <= heartbeatIntervalMs) {
    throw new OpenPrinterServerConfigurationError('heartbeatTimeoutMs must be greater than heartbeatIntervalMs.');
  }
  if (callbackTimeoutMs >= heartbeatTimeoutMs) {
    throw new OpenPrinterServerConfigurationError('callbackTimeoutMs must be less than heartbeatTimeoutMs.');
  }
  if (transportTimeoutMs >= heartbeatTimeoutMs) {
    throw new OpenPrinterServerConfigurationError('transportTimeoutMs must be less than heartbeatTimeoutMs.');
  }
  if (!isValidIdentifier(serverId)) {
    throw new OpenPrinterServerConfigurationError('serverId must be a valid OpenPrinter identifier.');
  }
  if (serverVersion.length < 1 || serverVersion.length > 256) {
    throw new OpenPrinterServerConfigurationError('serverVersion must contain between 1 and 256 characters.');
  }

  const brand = validateBrand(options.brand);
  const paths = validatePaths(options.paths);

  return {
    brand,
    authenticationTimeoutMs,
    callbackTimeoutMs,
    challengeTtlMs,
    handshakeTimeoutMs,
    heartbeatIntervalMs,
    heartbeatTimeoutMs,
    maxMessageBytes,
    paths,
    serverId,
    serverVersion,
    transportTimeoutMs,
  };
}

export function validateAcceptInput(input: AcceptOpenPrinterSessionInput): void {
  if (typeof input !== 'object' || input === null) {
    throw new OpenPrinterServerConfigurationError('accept() input must be an object.');
  }

  if (input.sessionId !== undefined && !isValidIdentifier(input.sessionId)) {
    throw new OpenPrinterServerConfigurationError('accept().sessionId must be a valid OpenPrinter identifier.');
  }
  if (typeof input.transport?.send !== 'function' || typeof input.transport.close !== 'function') {
    throw new OpenPrinterServerConfigurationError(
      'accept().transport must provide send(message) and close(request) functions.',
    );
  }
}

function validatePaths(paths: OpenPrinterServerOptions<unknown>['paths']): OpenPrinterServerPaths {
  const resolved = {
    discovery: paths?.discovery ?? OPENPRINTER_DISCOVERY_PATH,
    pairing: paths?.pairing ?? OPENPRINTER_PAIRING_PATH,
    gateway: paths?.gateway ?? OPENPRINTER_GATEWAY_PATH,
  };
  for (const [name, path] of Object.entries(resolved)) {
    if (!/^\/[A-Za-z0-9._~!$&'()*+,;=:@%/-]*$/.test(path) || path.includes('//')) {
      throw new OpenPrinterServerConfigurationError(
        `paths.${name} must be an absolute URL path without a query, fragment, or repeated slash.`,
      );
    }
  }
  if (new Set(Object.values(resolved)).size !== 3) {
    throw new OpenPrinterServerConfigurationError('Discovery, pairing, and gateway paths must be distinct.');
  }
  return Object.freeze(resolved);
}

export function transportCloseRequest(
  reason: OpenPrinterTransportCloseRequest['reason'],
  detail: string,
): OpenPrinterTransportCloseRequest {
  const bounded = boundedDetail(detail);
  return {
    reason,
    ...(bounded === undefined ? {} : { detail: bounded }),
  };
}

function validateBrand(brand: OpenPrinterBrandMetadata): OpenPrinterBrandMetadata {
  const name = brand?.name;
  if (typeof name !== 'string' || name.length < 1 || name.length > 256) {
    throw new OpenPrinterServerConfigurationError('brand.name must contain between 1 and 256 UTF-16 code units.');
  }

  const characters = [...name];
  const firstCodePoint = characters[0]?.codePointAt(0);
  const lastCodePoint = characters.at(-1)?.codePointAt(0);
  if (
    firstCodePoint === undefined ||
    lastCodePoint === undefined ||
    isBrandEdgeWhitespace(firstCodePoint) ||
    isBrandEdgeWhitespace(lastCodePoint) ||
    characters.some((character) => isBrandUnsafeCharacter(character.codePointAt(0) ?? 0))
  ) {
    throw new OpenPrinterServerConfigurationError(
      'brand.name must not contain leading or trailing whitespace, control characters, invalid Unicode, or direction-formatting controls.',
    );
  }

  return { name };
}

function isBrandUnsafeCharacter(codePoint: number): boolean {
  return (
    codePoint <= 0x1f ||
    (codePoint >= 0x7f && codePoint <= 0x9f) ||
    codePoint === 0x061c ||
    codePoint === 0x200e ||
    codePoint === 0x200f ||
    (codePoint >= 0x202a && codePoint <= 0x202e) ||
    (codePoint >= 0x2066 && codePoint <= 0x2069) ||
    (codePoint >= 0xd800 && codePoint <= 0xdfff)
  );
}

function isBrandEdgeWhitespace(codePoint: number): boolean {
  return (
    codePoint === 0x20 ||
    codePoint === 0xa0 ||
    codePoint === 0x1680 ||
    (codePoint >= 0x2000 && codePoint <= 0x200a) ||
    codePoint === 0x2028 ||
    codePoint === 0x2029 ||
    codePoint === 0x202f ||
    codePoint === 0x205f ||
    codePoint === 0x3000 ||
    codePoint === 0xfeff
  );
}

function requireIntegerInRange(name: string, value: number, minimum: number, maximum: number): void {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new OpenPrinterServerConfigurationError(`${name} must be an integer between ${minimum} and ${maximum}.`);
  }
}
