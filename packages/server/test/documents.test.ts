import { generateKeyPairSync, sign } from 'node:crypto';

import { encodeBase64Url, type PublicEd25519Jwk } from '@openprinter/protocol';
import { describe, expect, it } from 'vitest';

import { DOCUMENT_REQUEST_MAX_SKEW_MS, documentRequestSignaturePayload, verifyDocumentRequest } from '../src/index.js';

const SHA256 = 'ab'.repeat(32);

function keyPair(): {
  readonly privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'];
  readonly publicKey: PublicEd25519Jwk;
} {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const jwk = publicKey.export({ format: 'jwk' }) as { x: string };
  return { privateKey, publicKey: { kty: 'OKP', crv: 'Ed25519', x: jwk.x } };
}

function signed(
  privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'],
  overrides: Partial<{ agentId: string; jobId: string; timestampMs: number; pathAndQuery: string }> = {},
) {
  const input = {
    pathAndQuery: overrides.pathAndQuery ?? '/v1/documents/doc_1',
    agentId: overrides.agentId ?? 'agent_1',
    jobId: overrides.jobId ?? 'job_1',
    timestampMs: overrides.timestampMs ?? Date.parse('2026-07-28T10:00:00Z'),
    sha256: SHA256,
  };
  const signature = encodeBase64Url(
    sign(null, Buffer.from(documentRequestSignaturePayload(input), 'utf8'), privateKey),
  );
  return { ...input, signature };
}

const now = new Date('2026-07-28T10:00:30Z');

describe('document request signatures', () => {
  it('produces the canonical payload shared with the Rust signer', () => {
    expect(
      documentRequestSignaturePayload({
        pathAndQuery: '/d/job_1?v=1',
        agentId: 'agent_1',
        jobId: 'job_1',
        timestampMs: 1700,
        sha256: 'ab',
      }),
    ).toBe('OPENPRINTER-DOCUMENT-V1\nGET\n/d/job_1?v=1\nagent_1\njob_1\n1700\nab');
  });

  it('accepts a request signed by the registered agent key', () => {
    const { privateKey, publicKey } = keyPair();
    const request = signed(privateKey);
    expect(verifyDocumentRequest({ ...request, method: 'GET', publicKey, now })).toEqual({ ok: true });
  });

  it('rejects a signature for a different job', () => {
    const { privateKey, publicKey } = keyPair();
    const request = signed(privateKey, { jobId: 'job_1' });
    expect(verifyDocumentRequest({ ...request, jobId: 'job_2', method: 'GET', publicKey, now })).toEqual({
      ok: false,
      reason: 'invalid_signature',
    });
  });

  it('rejects a signature made by another key', () => {
    const signer = keyPair();
    const other = keyPair();
    const request = signed(signer.privateKey);
    expect(verifyDocumentRequest({ ...request, method: 'GET', publicKey: other.publicKey, now })).toEqual({
      ok: false,
      reason: 'invalid_signature',
    });
  });

  it('rejects requests outside the replay window', () => {
    const { privateKey, publicKey } = keyPair();
    const request = signed(privateKey);
    const later = new Date(now.getTime() + DOCUMENT_REQUEST_MAX_SKEW_MS + 1_000);
    expect(verifyDocumentRequest({ ...request, method: 'GET', publicKey, now: later })).toEqual({
      ok: false,
      reason: 'stale_timestamp',
    });
  });

  it('accepts only GET document requests', () => {
    const { privateKey, publicKey } = keyPair();
    const request = signed(privateKey);
    expect(verifyDocumentRequest({ ...request, method: 'POST', publicKey, now })).toEqual({
      ok: false,
      reason: 'method_not_allowed',
    });
  });

  it('treats malformed signatures as invalid rather than throwing', () => {
    const { publicKey } = keyPair();
    const request = signed(keyPair().privateKey);
    expect(verifyDocumentRequest({ ...request, signature: '!!!', method: 'GET', publicKey, now })).toEqual({
      ok: false,
      reason: 'invalid_signature',
    });
  });
});
