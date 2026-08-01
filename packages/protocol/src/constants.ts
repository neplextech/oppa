/**
 * The only wire protocol version understood by this release.
 *
 * Peers still advertise supported versions during the hello exchange so a
 * future release can negotiate without changing the envelope.
 */
export const PROTOCOL_VERSION = '1' as const;

/** Default discovery endpoint relative to an OpenPrinter server base URL. */
export const OPENPRINTER_DISCOVERY_PATH = '/.well-known/openprinter' as const;

/** Default one-time pairing endpoint. */
export const OPENPRINTER_PAIRING_PATH = '/openprinter/pair' as const;

/** Default authenticated agent gateway endpoint. */
export const OPENPRINTER_GATEWAY_PATH = '/.well-known/openprinter/gateway' as const;

/** Authentication method implemented by OpenPrinter protocol version 1. */
export const OPENPRINTER_AUTHENTICATION_METHOD = 'pairing-code-ed25519' as const;

/** Signature algorithm implemented by OpenPrinter protocol version 1. */
export const OPENPRINTER_SIGNATURE_ALGORITHM = 'Ed25519' as const;

/** WebSocket application close codes used during gateway authentication. */
export const OPENPRINTER_AUTH_CLOSE_CODES = Object.freeze({
  rejected: 4401,
  timeout: 4408,
  protocolError: 4400,
});

/** The JSON Schema identifier for the complete OpenPrinter v1 wire contract. */
export const PROTOCOL_SCHEMA_ID = 'https://openprinter.dev/schema/openprinter-v1.schema.json';

/** Maximum encoded authentication frame size. */
export const MAX_AUTH_MESSAGE_BYTES = 16 * 1024;

/** Maximum UTF-8 size accepted by any protocol decoder. */
export const MAX_WIRE_MESSAGE_BYTES = 2 * 1024 * 1024;

/** Maximum number of primitives in one structured print document. */
export const MAX_DOCUMENT_SECTIONS = 256;

/** Maximum number of printers accepted in an inventory snapshot. */
export const MAX_PRINTERS_PER_INVENTORY = 512;

/** Maximum encoded image length accepted in a document. */
export const MAX_IMAGE_BASE64_LENGTH = 1_398_104;

/** Maximum number of opaque string metadata entries carried by a job. */
export const MAX_METADATA_ENTRIES = 32;

/**
 * Public protocol limits for queueing, transport, and UI diagnostics.
 *
 * Consumers should enforce the wire limit before buffering an entire message.
 */
export const PROTOCOL_LIMITS = Object.freeze({
  maxWireMessageBytes: MAX_WIRE_MESSAGE_BYTES,
  maxDocumentSections: MAX_DOCUMENT_SECTIONS,
  maxPrintersPerInventory: MAX_PRINTERS_PER_INVENTORY,
  maxImageBase64Length: MAX_IMAGE_BASE64_LENGTH,
  maxMetadataEntries: MAX_METADATA_ENTRIES,
});
