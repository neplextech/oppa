import { createPublicKey, verify } from 'node:crypto';

import { decodeBase64Url, type PublicEd25519Jwk } from '@openprinter/protocol';

/** Domain-separation tag at the start of every signed document request. */
export const DOCUMENT_SIGNATURE_CONTEXT = 'OPENPRINTER-DOCUMENT-V1';

/** Largest clock skew accepted between an agent request and this server. */
export const DOCUMENT_REQUEST_MAX_SKEW_MS = 5 * 60 * 1000;

/** Request headers an agent sends when it retrieves a job document. */
export const DOCUMENT_REQUEST_HEADERS = Object.freeze({
  agentId: 'x-openprinter-agent-id',
  jobId: 'x-openprinter-job-id',
  timestamp: 'x-openprinter-timestamp',
  signature: 'x-openprinter-signature',
});

/** Identity and integrity fields bound into one document request signature. */
export interface DocumentRequestSignatureInput {
  /** Path and query exactly as requested, including the leading slash. */
  readonly pathAndQuery: string;
  /** Stable agent identity from the paired credential. */
  readonly agentId: string;
  /** Durable job identity whose document is requested. */
  readonly jobId: string;
  /** Agent-supplied Unix time in milliseconds. */
  readonly timestampMs: number;
  /** Lowercase hexadecimal SHA-256 digest declared for the document. */
  readonly sha256: string;
}

/** Values read from one incoming document request. */
export interface DocumentRequestVerificationInput extends DocumentRequestSignatureInput {
  /** HTTP method. Only `GET` is valid for documents. */
  readonly method: string;
  /** Unpadded base64url Ed25519 signature from `X-OpenPrinter-Signature`. */
  readonly signature: string;
  /** Public key registered for the agent identity. */
  readonly publicKey: PublicEd25519Jwk;
  /** Current time used to bound replay. Defaults to `new Date()`. */
  readonly now?: Date;
  /** Accepted clock skew. Defaults to five minutes. */
  readonly maxSkewMs?: number;
}

/** Reasons a document request is refused. Never includes signature or key material. */
export type DocumentRequestRejection = 'method_not_allowed' | 'stale_timestamp' | 'invalid_signature';

/** Outcome of verifying one document request. */
export type DocumentRequestVerification =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: DocumentRequestRejection };

/**
 * Builds the exact string an agent signs to retrieve a job document.
 *
 * The lines are LF-separated and appear in this order: context, method, path and query, agent id,
 * job id, timestamp, and SHA-256 digest. The Rust `oppa-documents` crate produces the same bytes.
 */
export function documentRequestSignaturePayload(input: DocumentRequestSignatureInput): string {
  return [
    DOCUMENT_SIGNATURE_CONTEXT,
    'GET',
    input.pathAndQuery,
    input.agentId,
    input.jobId,
    String(input.timestampMs),
    input.sha256,
  ].join('\n');
}

/**
 * Verifies that an agent authorized retrieval of one job document.
 *
 * Callers must resolve `publicKey` from their own credential store for `agentId`, and must still
 * confirm that the job belongs to that agent before serving bytes.
 */
export function verifyDocumentRequest(input: DocumentRequestVerificationInput): DocumentRequestVerification {
  if (input.method !== 'GET') {
    return { ok: false, reason: 'method_not_allowed' };
  }
  const now = (input.now ?? new Date()).getTime();
  const skew = Math.abs(now - input.timestampMs);
  if (!Number.isSafeInteger(input.timestampMs) || skew > (input.maxSkewMs ?? DOCUMENT_REQUEST_MAX_SKEW_MS)) {
    return { ok: false, reason: 'stale_timestamp' };
  }

  const payload = Buffer.from(documentRequestSignaturePayload(input), 'utf8');
  try {
    const publicKey = createPublicKey({ key: input.publicKey, format: 'jwk' });
    const valid = verify(null, payload, publicKey, decodeBase64Url(input.signature));
    return valid ? { ok: true } : { ok: false, reason: 'invalid_signature' };
  } catch {
    return { ok: false, reason: 'invalid_signature' };
  }
}
