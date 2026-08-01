import { Type, type Static } from '@sinclair/typebox';

import { OPENPRINTER_AUTHENTICATION_METHOD, OPENPRINTER_SIGNATURE_ALGORITHM, PROTOCOL_VERSION } from '../constants.js';
import { IdentifierSchema, TimestampSchema } from './common.js';
import { OpenPrinterBrandNameSchema } from './messages.js';

/** Unpadded RFC 4648 base64url data. */
export const Base64UrlSchema = Type.String({
  minLength: 1,
  maxLength: 4096,
  pattern: '^[A-Za-z0-9_-]+$',
});

/** Public subset of an Ed25519 JWK. */
export const PublicEd25519JwkSchema = Type.Object(
  {
    kty: Type.Literal('OKP'),
    crv: Type.Literal('Ed25519'),
    x: Type.String({ minLength: 43, maxLength: 43, pattern: '^[A-Za-z0-9_-]{43}$' }),
  },
  { additionalProperties: false },
);
export type PublicEd25519Jwk = Static<typeof PublicEd25519JwkSchema>;

/** Relative or absolute endpoint advertised by discovery. */
export const OpenPrinterEndpointSchema = Type.String({
  minLength: 1,
  maxLength: 2048,
  pattern: '^(?:/[^\\s?#]*|(?:https?|wss?)://[^\\s]+)$',
});

/** Server discovery document returned by `/.well-known/openprinter`. */
export const OpenPrinterDiscoveryDocumentSchema = Type.Object(
  {
    protocolVersion: Type.Literal(PROTOCOL_VERSION),
    server: Type.Object(
      {
        id: IdentifierSchema,
        name: OpenPrinterBrandNameSchema,
        version: Type.String({ minLength: 1, maxLength: 256 }),
      },
      { additionalProperties: false },
    ),
    endpoints: Type.Object(
      {
        pairing: OpenPrinterEndpointSchema,
        gateway: OpenPrinterEndpointSchema,
      },
      { additionalProperties: false },
    ),
    authentication: Type.Object(
      {
        method: Type.Literal(OPENPRINTER_AUTHENTICATION_METHOD),
        challengeTtlSeconds: Type.Integer({ minimum: 5, maximum: 300 }),
      },
      { additionalProperties: false },
    ),
  },
  { additionalProperties: false, title: 'OpenPrinterDiscoveryDocument' },
);
export type OpenPrinterDiscoveryDocument = Static<typeof OpenPrinterDiscoveryDocumentSchema>;

/** Pairing request body sent by an agent after local key generation. */
export const OpenPrinterPairingRequestSchema = Type.Object(
  {
    protocolVersion: Type.Literal(PROTOCOL_VERSION),
    code: Type.String({ minLength: 4, maxLength: 64, pattern: '^[A-Za-z0-9-]+$' }),
    agent: Type.Object(
      {
        name: Type.String({ minLength: 1, maxLength: 128 }),
        version: Type.String({ minLength: 1, maxLength: 256 }),
        platform: Type.String({ minLength: 1, maxLength: 64 }),
        installationId: IdentifierSchema,
      },
      { additionalProperties: false },
    ),
    credential: Type.Object(
      {
        algorithm: Type.Literal(OPENPRINTER_SIGNATURE_ALGORITHM),
        publicKey: PublicEd25519JwkSchema,
      },
      { additionalProperties: false },
    ),
  },
  { additionalProperties: false, title: 'OpenPrinterPairingRequest' },
);
export type OpenPrinterPairingRequest = Static<typeof OpenPrinterPairingRequestSchema>;

/** Successful pairing response containing only non-secret identifiers. */
export const OpenPrinterPairingResponseSchema = Type.Object(
  {
    agentId: IdentifierSchema,
    keyId: IdentifierSchema,
    serverId: IdentifierSchema,
    pairedAt: TimestampSchema,
  },
  { additionalProperties: false, title: 'OpenPrinterPairingResponse' },
);
export type OpenPrinterPairingResponse = Static<typeof OpenPrinterPairingResponseSchema>;

export const PairingErrorCodeSchema = Type.Union([
  Type.Literal('pairing_code_invalid'),
  Type.Literal('pairing_code_expired'),
  Type.Literal('pairing_code_consumed'),
  Type.Literal('pairing_rate_limited'),
  Type.Literal('invalid_public_key'),
  Type.Literal('unsupported_protocol_version'),
  Type.Literal('unsupported_authentication_method'),
  Type.Literal('server_error'),
]);
export type PairingErrorCode = Static<typeof PairingErrorCodeSchema>;

/** Machine-readable pairing failure envelope. */
export const OpenPrinterErrorEnvelopeSchema = Type.Object(
  {
    error: Type.Object(
      {
        code: PairingErrorCodeSchema,
        message: Type.String({ minLength: 1, maxLength: 512 }),
      },
      { additionalProperties: false },
    ),
  },
  { additionalProperties: false },
);
export type OpenPrinterErrorEnvelope = Static<typeof OpenPrinterErrorEnvelopeSchema>;

/** Socket-bound opaque challenge sent before any normal gateway traffic. */
export const GatewayAuthenticationChallengeSchema = Type.Object(
  {
    type: Type.Literal('auth.challenge'),
    challengeId: IdentifierSchema,
    payload: Base64UrlSchema,
    expiresAt: TimestampSchema,
  },
  { additionalProperties: false },
);
export type GatewayAuthenticationChallenge = Static<typeof GatewayAuthenticationChallengeSchema>;

/** Agent proof over the exact decoded challenge payload bytes. */
export const GatewayAuthenticationResponseSchema = Type.Object(
  {
    type: Type.Literal('auth.response'),
    challengeId: IdentifierSchema,
    agentId: IdentifierSchema,
    keyId: IdentifierSchema,
    algorithm: Type.Literal(OPENPRINTER_SIGNATURE_ALGORITHM),
    signature: Type.String({ minLength: 86, maxLength: 86, pattern: '^[A-Za-z0-9_-]{86}$' }),
  },
  { additionalProperties: false },
);
export type GatewayAuthenticationResponse = Static<typeof GatewayAuthenticationResponseSchema>;

/** Confirmation that normal OpenPrinter gateway traffic may begin. */
export const GatewayAuthenticationAcceptedSchema = Type.Object(
  {
    type: Type.Literal('auth.accepted'),
    sessionId: IdentifierSchema,
    agentId: IdentifierSchema,
    heartbeatIntervalMs: Type.Integer({ minimum: 5000, maximum: 300000 }),
  },
  { additionalProperties: false },
);
export type GatewayAuthenticationAccepted = Static<typeof GatewayAuthenticationAcceptedSchema>;

export const GatewayAuthenticationFailureCodeSchema = Type.Union([
  Type.Literal('challenge_expired'),
  Type.Literal('challenge_invalid'),
  Type.Literal('challenge_consumed'),
  Type.Literal('credential_not_found'),
  Type.Literal('credential_revoked'),
  Type.Literal('invalid_signature'),
  Type.Literal('unsupported_algorithm'),
  Type.Literal('authentication_timeout'),
]);
export type GatewayAuthenticationFailureCode = Static<typeof GatewayAuthenticationFailureCodeSchema>;

/** Bounded failure sent before the server closes an unauthenticated socket. */
export const GatewayAuthenticationRejectedSchema = Type.Object(
  {
    type: Type.Literal('auth.rejected'),
    code: GatewayAuthenticationFailureCodeSchema,
    message: Type.String({ minLength: 1, maxLength: 512 }),
  },
  { additionalProperties: false },
);
export type GatewayAuthenticationRejected = Static<typeof GatewayAuthenticationRejectedSchema>;

export const GatewayAuthenticationServerMessageSchema = Type.Union([
  GatewayAuthenticationChallengeSchema,
  GatewayAuthenticationAcceptedSchema,
  GatewayAuthenticationRejectedSchema,
]);
export type GatewayAuthenticationServerMessage = Static<typeof GatewayAuthenticationServerMessageSchema>;

/** Complete JSON Schema source including REST and gateway authentication contracts. */
export const OpenPrinterAuthenticationSchema = Type.Union(
  [
    OpenPrinterDiscoveryDocumentSchema,
    OpenPrinterPairingRequestSchema,
    OpenPrinterPairingResponseSchema,
    OpenPrinterErrorEnvelopeSchema,
    GatewayAuthenticationChallengeSchema,
    GatewayAuthenticationResponseSchema,
    GatewayAuthenticationAcceptedSchema,
    GatewayAuthenticationRejectedSchema,
  ],
  { title: 'OpenPrinterAuthenticationContract' },
);
