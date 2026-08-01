import { randomBytes, randomUUID } from 'node:crypto';

import {
  OPENPRINTER_AUTHENTICATION_METHOD,
  OPENPRINTER_AUTH_CLOSE_CODES,
  PROTOCOL_VERSION,
  ProtocolError,
  parsePairingRequest,
  type OpenPrinterErrorEnvelope,
  type OpenPrinterPairingResponse,
} from '@openprinter/protocol';

import { validateAcceptInput, validateOptions } from './internal.js';
import { OpenPrinterSessionImplementation } from './session.js';
import { InMemoryAgentCredentialStore, InMemoryPairingCodeStore } from './storage.js';
import type {
  AcceptOpenPrinterSessionInput,
  AgentCredentialRecord,
  AgentCredentialStore,
  CreatePairingCodeInput,
  CreatedPairingCode,
  OpenPrinterGatewaySocket,
  OpenPrinterServer,
  OpenPrinterServerOptions,
  OpenPrinterSession,
  PairingAttemptContext,
  PairingCodeStore,
} from './types.js';

const DEFAULT_PAIRING_EXPIRY_MS = 5 * 60_000;
const PAIRING_CODE_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

/** Create a framework-neutral discovery, pairing, and authenticated session SDK. */
export function createOpenPrinterServer<Metadata>(
  options: OpenPrinterServerOptions<Metadata>,
): OpenPrinterServer<Metadata> {
  const resolved = validateOptions(options);
  const pairingCodeStore: PairingCodeStore<Metadata> =
    options.pairingCodeStore ?? new InMemoryPairingCodeStore<Metadata>();
  const credentialStore: AgentCredentialStore<Metadata> =
    options.credentialStore ?? new InMemoryAgentCredentialStore<Metadata>();

  const server: OpenPrinterServer<Metadata> = {
    paths: resolved.paths,

    discover() {
      return Promise.resolve({
        protocolVersion: PROTOCOL_VERSION,
        server: {
          id: resolved.serverId,
          name: resolved.brand.name,
          version: resolved.serverVersion,
        },
        endpoints: {
          pairing: resolved.paths.pairing,
          gateway: resolved.paths.gateway,
        },
        authentication: {
          method: OPENPRINTER_AUTHENTICATION_METHOD,
          challengeTtlSeconds: Math.floor(resolved.challengeTtlMs / 1000),
        },
      });
    },

    async createPairingCode(input: CreatePairingCodeInput<Metadata> = {}): Promise<CreatedPairingCode<Metadata>> {
      const expiresInMs = input.expiresInMs ?? DEFAULT_PAIRING_EXPIRY_MS;
      if (!Number.isInteger(expiresInMs) || expiresInMs < 10_000 || expiresInMs > 24 * 60 * 60_000) {
        throw new TypeError('expiresInMs must be an integer between 10000 and 86400000.');
      }
      const createdAt = new Date();
      const record = await pairingCodeStore.create({
        code: generatePairingCode(),
        createdAt,
        expiresAt: new Date(createdAt.getTime() + expiresInMs),
        ...(input.metadata === undefined ? {} : { metadata: input.metadata }),
      });
      return record;
    },

    async pair(input: unknown, context: PairingAttemptContext = {}) {
      const rateLimit = await options.checkPairingRateLimit?.(context);
      if (rateLimit === false || (typeof rateLimit === 'object' && !rateLimit.allowed)) {
        return pairingError('pairing_rate_limited', 'Too many pairing attempts. Try again later.');
      }

      let request;
      try {
        request = parsePairingRequest(input);
      } catch (error) {
        if (error instanceof ProtocolError && error.code === 'unsupported_protocol_version') {
          return pairingError('unsupported_protocol_version', 'This server does not support that protocol version.');
        }
        return pairingError('invalid_public_key', 'The pairing request or public key is invalid.');
      }

      let response: OpenPrinterPairingResponse | null = null;
      try {
        const result = await pairingCodeStore.consume(request.code, new Date(), async (grant) => {
          const pairedAt = new Date();
          const credential: AgentCredentialRecord<Metadata> = {
            agentId: `agt_${randomUUID().replaceAll('-', '')}`,
            keyId: `key_${randomUUID().replaceAll('-', '')}`,
            algorithm: 'Ed25519',
            publicKey: request.credential.publicKey,
            createdAt: pairedAt,
            revokedAt: null,
            ...(grant.metadata === undefined ? {} : { metadata: grant.metadata }),
          };
          await credentialStore.create(credential);
          response = {
            agentId: credential.agentId,
            keyId: credential.keyId,
            serverId: resolved.serverId,
            pairedAt: pairedAt.toISOString(),
          };
        });

        switch (result) {
          case 'consumed':
            return response ?? pairingError('server_error', 'The server could not complete pairing.');
          case 'expired':
            return pairingError('pairing_code_expired', 'The pairing code has expired.');
          case 'already-consumed':
            return pairingError('pairing_code_consumed', 'The pairing code has already been used.');
          case 'invalid':
            return pairingError('pairing_code_invalid', 'The pairing code is invalid.');
        }
      } catch {
        return pairingError('server_error', 'The server could not complete pairing.');
      }
    },

    async revokeCredential(agentId: string, keyId: string) {
      await credentialStore.revoke(agentId, keyId, new Date());
    },

    accept(input: AcceptOpenPrinterSessionInput): OpenPrinterSession<Metadata> {
      validateAcceptInput(input);
      return new OpenPrinterSessionImplementation(
        options,
        resolved,
        input,
        input.sessionId ?? `session_${randomUUID().replaceAll('-', '')}`,
        credentialStore,
      );
    },

    handleGatewayConnection(socket: OpenPrinterGatewaySocket): OpenPrinterSession<Metadata> {
      const session = server.accept({
        transport: {
          send(message) {
            return sendSocketMessage(socket, message);
          },
          close(request) {
            const closeCode =
              request.reason !== 'authentication-failed'
                ? OPENPRINTER_AUTH_CLOSE_CODES.protocolError
                : request.detail === 'authentication_timeout'
                  ? OPENPRINTER_AUTH_CLOSE_CODES.timeout
                  : OPENPRINTER_AUTH_CLOSE_CODES.rejected;
            socket.close(closeCode, request.detail ?? request.reason);
          },
        },
      });
      socket.on('message', (data) => {
        if (typeof data === 'string' || data instanceof Uint8Array) {
          void session.receive(data);
        } else {
          void session.receive(String(data));
        }
      });
      socket.on('close', (_code, reason) => {
        const detail = socketDetail(reason);
        void session.transportClosed({
          reason: 'peer-closed',
          ...(detail === undefined ? {} : { detail }),
        });
      });
      socket.on('error', (error) => {
        void session.transportClosed({ reason: 'transport-error', detail: error.name });
      });
      return session;
    },
  };

  return server;
}

function generatePairingCode(): string {
  const bytes = randomBytes(8);
  let value = '';
  for (const byte of bytes) {
    value += PAIRING_CODE_ALPHABET[byte % PAIRING_CODE_ALPHABET.length];
  }
  return `${value.slice(0, 4)}-${value.slice(4)}`;
}

function pairingError(code: OpenPrinterErrorEnvelope['error']['code'], message: string): OpenPrinterErrorEnvelope {
  return { error: { code, message } };
}

function sendSocketMessage(socket: OpenPrinterGatewaySocket, message: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const callback = (error?: Error | null) => {
      settled = true;
      if (error == null) resolve();
      else reject(error);
    };
    try {
      socket.send(message, callback);
      if (socket.send.length < 2 && !settled) resolve();
    } catch (error) {
      reject(error instanceof Error ? error : new Error('The gateway socket rejected the message.'));
    }
  });
}

function socketDetail(value: unknown): string | undefined {
  if (typeof value === 'string') return value;
  if (value instanceof Uint8Array) return new TextDecoder().decode(value);
  return undefined;
}
