/**
 * The only wire protocol version understood by this release.
 *
 * Peers still advertise supported versions during the hello exchange so a
 * future release can negotiate without changing the envelope.
 */
export const PROTOCOL_VERSION = 1 as const;

/** The JSON Schema identifier for the complete OpenPrinter v1 wire contract. */
export const PROTOCOL_SCHEMA_ID = 'https://openprinter.dev/schema/openprinter-v1.schema.json';

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
