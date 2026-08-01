import { generateKeyPairSync, sign } from 'node:crypto';

import {
  PROTOCOL_VERSION,
  decodeBase64Url,
  decodeGatewayAuthenticationServerMessage,
  decodeServerMessage,
  encodeGatewayAuthenticationResponse,
  type GatewayAuthenticationChallenge,
  type OpenPrinterPairingRequest,
} from '@openprinter/protocol';
import { describe, expect, it, vi } from 'vitest';

import {
  OpenPrinterServerConfigurationError,
  InMemoryPairingCodeStore,
  createOpenPrinterServer,
  type OpenPrinterServer,
  type OpenPrinterTransportCloseRequest,
} from '../src/index.js';

interface TestCredential {
  readonly privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'];
  readonly publicKey: OpenPrinterPairingRequest['credential']['publicKey'];
}

interface TestTransport {
  readonly sent: string[];
  readonly closes: OpenPrinterTransportCloseRequest[];
  readonly transport: {
    send(message: string): void;
    close(request: OpenPrinterTransportCloseRequest): void;
  };
}

function credential(): TestCredential {
  const pair = generateKeyPairSync('ed25519');
  const jwk = pair.publicKey.export({ format: 'jwk' });
  return {
    privateKey: pair.privateKey,
    publicKey: { kty: 'OKP', crv: 'Ed25519', x: jwk.x! },
  };
}

function transport(): TestTransport {
  const sent: string[] = [];
  const closes: OpenPrinterTransportCloseRequest[] = [];
  return {
    sent,
    closes,
    transport: {
      send(message) {
        sent.push(message);
      },
      close(request) {
        closes.push(request);
      },
    },
  };
}

function server(
  overrides: Parameters<typeof createOpenPrinterServer<Record<string, string>>>[0] = {
    brand: { name: 'Test Print Service' },
  },
) {
  const { brand, ...rest } = overrides;
  return createOpenPrinterServer<Record<string, string>>({
    serverId: 'test-service',
    serverVersion: '1.2.3',
    heartbeatIntervalMs: 60_000,
    heartbeatTimeoutMs: 120_000,
    ...rest,
    brand,
  });
}

function pairingRequest(
  code: string,
  key: TestCredential,
  installationId = 'installation-01',
): OpenPrinterPairingRequest {
  return {
    protocolVersion: PROTOCOL_VERSION,
    code,
    agent: {
      name: 'Front Counter',
      version: '1.0.0',
      platform: 'test',
      installationId,
    },
    credential: { algorithm: 'Ed25519', publicKey: key.publicKey },
  };
}

async function waitFor(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error('Timed out waiting for test state.');
}

async function pairAgent(openprinter: OpenPrinterServer<Record<string, string>>, key: TestCredential) {
  const pairing = await openprinter.createPairingCode({ metadata: { tenantId: 'tenant-1' } });
  const result = await openprinter.pair(pairingRequest(pairing.code.toLowerCase(), key));
  if ('error' in result) throw new Error(result.error.code);
  return result;
}

async function authenticate(
  openprinter: OpenPrinterServer<Record<string, string>>,
  key: TestCredential,
  agent: Awaited<ReturnType<typeof pairAgent>>,
) {
  const wire = transport();
  const session = openprinter.accept({ transport: wire.transport });
  await waitFor(() => wire.sent.length === 1);
  const challenge = decodeGatewayAuthenticationServerMessage(wire.sent[0]!) as GatewayAuthenticationChallenge;
  const signature = sign(null, decodeBase64Url(challenge.payload), key.privateKey).toString('base64url');
  await session.receive(
    encodeGatewayAuthenticationResponse({
      type: 'auth.response',
      challengeId: challenge.challengeId,
      agentId: agent.agentId,
      keyId: agent.keyId,
      algorithm: 'Ed25519',
      signature,
    }),
  );
  expect(decodeGatewayAuthenticationServerMessage(wire.sent[1]!)).toMatchObject({
    type: 'auth.accepted',
    agentId: agent.agentId,
  });
  return { session, wire };
}

function hello(agentId: string) {
  return JSON.stringify({
    protocolVersion: PROTOCOL_VERSION,
    messageId: 'hello-01',
    sentAt: '2026-08-01T09:00:00.000Z',
    type: 'agent.hello',
    payload: {
      agentId,
      agentVersion: '1.0.0',
      productId: 'oppa',
      productVersion: '1.0.0',
      supportedProtocolVersions: [PROTOCOL_VERSION],
    },
  });
}

describe('discovery and pairing', () => {
  it('returns default relative paths and supports custom paths', async () => {
    const defaults = server();
    expect(await defaults.discover()).toMatchObject({
      protocolVersion: '1',
      endpoints: {
        pairing: '/openprinter/pair',
        gateway: '/.well-known/openprinter/gateway',
      },
      authentication: { method: 'pairing-code-ed25519', challengeTtlSeconds: 30 },
    });

    const custom = server({
      brand: { name: 'Custom' },
      paths: { discovery: '/printer/discovery', pairing: '/printer/pair', gateway: '/printer/gateway' },
    });
    expect(custom.paths).toEqual({
      discovery: '/printer/discovery',
      pairing: '/printer/pair',
      gateway: '/printer/gateway',
    });
    expect((await custom.discover()).endpoints.gateway).toBe('/printer/gateway');
  });

  it('rejects invalid or colliding paths', () => {
    expect(() => server({ brand: { name: 'Bad' }, paths: { pairing: '/same', gateway: '/same' } })).toThrow(
      OpenPrinterServerConfigurationError,
    );
    expect(() => server({ brand: { name: 'Bad' }, paths: { pairing: 'relative' } })).toThrow(
      OpenPrinterServerConfigurationError,
    );
  });

  it('creates case-insensitive, expiring, single-use pairing codes', async () => {
    const openprinter = server();
    const key = credential();
    expect(await openprinter.pair(pairingRequest('ZZZZ-ZZZZ', key))).toMatchObject({
      error: { code: 'pairing_code_invalid' },
    });
    const pairing = await openprinter.createPairingCode({ expiresInMs: 60_000 });
    expect(pairing.code).toMatch(/^[A-HJ-NP-Z2-9]{4}-[A-HJ-NP-Z2-9]{4}$/);
    const first = await openprinter.pair(pairingRequest(pairing.code.toLowerCase(), key));
    expect(first).toMatchObject({ serverId: 'test-service' });
    expect(await openprinter.pair(pairingRequest(pairing.code, credential()))).toEqual({
      error: { code: 'pairing_code_consumed', message: 'The pairing code has already been used.' },
    });
  });

  it('atomically permits only one concurrent redemption', async () => {
    const openprinter = server();
    const pairing = await openprinter.createPairingCode();
    const results = await Promise.all([
      openprinter.pair(pairingRequest(pairing.code, credential(), 'installation-a')),
      openprinter.pair(pairingRequest(pairing.code, credential(), 'installation-b')),
    ]);
    expect(results.filter((result) => !('error' in result))).toHaveLength(1);
    expect(results.filter((result) => 'error' in result)[0]).toMatchObject({
      error: { code: 'pairing_code_consumed' },
    });
  });

  it('expires pairing grants without invoking credential registration', async () => {
    const store = new InMemoryPairingCodeStore<unknown>();
    const register = vi.fn(() => Promise.resolve());
    await store.create({
      code: 'ABCD-EFGH',
      createdAt: new Date('2026-08-01T09:00:00.000Z'),
      expiresAt: new Date('2026-08-01T09:05:00.000Z'),
    });
    await expect(store.consume('abcd-efgh', new Date('2026-08-01T09:05:00.001Z'), register)).resolves.toBe('expired');
    expect(register).not.toHaveBeenCalled();
  });

  it('rejects invalid keys and invokes the rate-limit policy', async () => {
    const checkPairingRateLimit = vi.fn(() => false);
    const limited = server({ brand: { name: 'Limited' }, checkPairingRateLimit });
    expect(await limited.pair({})).toMatchObject({ error: { code: 'pairing_rate_limited' } });
    expect(checkPairingRateLimit).toHaveBeenCalledOnce();

    const openprinter = server();
    const pairing = await openprinter.createPairingCode();
    expect(
      await openprinter.pair({
        ...pairingRequest(pairing.code, credential()),
        credential: { algorithm: 'Ed25519', publicKey: { kty: 'OKP', crv: 'Ed25519', x: 'short' } },
      }),
    ).toMatchObject({ error: { code: 'invalid_public_key' } });
  });
});

describe('gateway authentication', () => {
  it('authenticates before hello and preserves authenticated session behavior', async () => {
    const onAgentConnected = vi.fn();
    const openprinter = server({ brand: { name: 'Test' }, onAgentConnected });
    const key = credential();
    const paired = await pairAgent(openprinter, key);
    const { session, wire } = await authenticate(openprinter, key, paired);
    expect(session.state).toBe('handshaking');
    expect(session.identity).toMatchObject({ agentId: paired.agentId, metadata: { tenantId: 'tenant-1' } });
    expect(onAgentConnected).not.toHaveBeenCalled();

    await session.receive(hello(paired.agentId));
    expect(session.state).toBe('connected');
    expect(decodeServerMessage(wire.sent[2]!)).toMatchObject({ type: 'server.hello' });
    expect(onAgentConnected).toHaveBeenCalledOnce();

    const delivery = await session.requestPrinters();
    expect(delivery).toMatchObject({ ok: true, agentId: paired.agentId });
  });

  it('rejects normal protocol traffic before authentication', async () => {
    const openprinter = server();
    const wire = transport();
    const session = openprinter.accept({ transport: wire.transport });
    await waitFor(() => wire.sent.length === 1);
    await session.receive(hello('agent-01'));
    expect(decodeGatewayAuthenticationServerMessage(wire.sent[1]!)).toMatchObject({
      type: 'auth.rejected',
      code: 'challenge_invalid',
    });
    expect(session.state).toBe('closed');
    expect(wire.closes[0]).toMatchObject({ reason: 'authentication-failed' });
  });

  it('rejects wrong signatures and revoked credentials without lifecycle hooks', async () => {
    const onAgentConnected = vi.fn();
    const openprinter = server({ brand: { name: 'Test' }, onAgentConnected });
    const key = credential();
    const paired = await pairAgent(openprinter, key);
    const wire = transport();
    const session = openprinter.accept({ transport: wire.transport });
    await waitFor(() => wire.sent.length === 1);
    const challenge = decodeGatewayAuthenticationServerMessage(wire.sent[0]!) as GatewayAuthenticationChallenge;
    const wrongSignature = sign(null, decodeBase64Url(challenge.payload), credential().privateKey).toString(
      'base64url',
    );
    await session.receive(
      encodeGatewayAuthenticationResponse({
        type: 'auth.response',
        challengeId: challenge.challengeId,
        agentId: paired.agentId,
        keyId: paired.keyId,
        algorithm: 'Ed25519',
        signature: wrongSignature,
      }),
    );
    expect(decodeGatewayAuthenticationServerMessage(wire.sent[1]!)).toMatchObject({ code: 'invalid_signature' });
    expect(onAgentConnected).not.toHaveBeenCalled();

    await openprinter.revokeCredential(paired.agentId, paired.keyId);
    const revokedWire = transport();
    const revokedSession = openprinter.accept({ transport: revokedWire.transport });
    await waitFor(() => revokedWire.sent.length === 1);
    const revokedChallenge = decodeGatewayAuthenticationServerMessage(
      revokedWire.sent[0]!,
    ) as GatewayAuthenticationChallenge;
    await revokedSession.receive(
      encodeGatewayAuthenticationResponse({
        type: 'auth.response',
        challengeId: revokedChallenge.challengeId,
        agentId: paired.agentId,
        keyId: paired.keyId,
        algorithm: 'Ed25519',
        signature: sign(null, decodeBase64Url(revokedChallenge.payload), key.privateKey).toString('base64url'),
      }),
    );
    expect(decodeGatewayAuthenticationServerMessage(revokedWire.sent[1]!)).toMatchObject({
      code: 'credential_revoked',
    });
  });

  it('rejects unknown credentials and a challenge replayed on another socket', async () => {
    const openprinter = server();
    const key = credential();
    const paired = await pairAgent(openprinter, key);

    const unknownWire = transport();
    const unknownSession = openprinter.accept({ transport: unknownWire.transport });
    await waitFor(() => unknownWire.sent.length === 1);
    const unknownChallenge = decodeGatewayAuthenticationServerMessage(
      unknownWire.sent[0]!,
    ) as GatewayAuthenticationChallenge;
    await unknownSession.receive(
      encodeGatewayAuthenticationResponse({
        type: 'auth.response',
        challengeId: unknownChallenge.challengeId,
        agentId: 'unknown-agent',
        keyId: 'unknown-key',
        algorithm: 'Ed25519',
        signature: sign(null, decodeBase64Url(unknownChallenge.payload), key.privateKey).toString('base64url'),
      }),
    );
    expect(decodeGatewayAuthenticationServerMessage(unknownWire.sent[1]!)).toMatchObject({
      type: 'auth.rejected',
      code: 'credential_not_found',
    });

    const sourceWire = transport();
    openprinter.accept({ transport: sourceWire.transport });
    const targetWire = transport();
    const targetSession = openprinter.accept({ transport: targetWire.transport });
    await waitFor(() => sourceWire.sent.length === 1 && targetWire.sent.length === 1);
    const sourceChallenge = decodeGatewayAuthenticationServerMessage(
      sourceWire.sent[0]!,
    ) as GatewayAuthenticationChallenge;
    await targetSession.receive(
      encodeGatewayAuthenticationResponse({
        type: 'auth.response',
        challengeId: sourceChallenge.challengeId,
        agentId: paired.agentId,
        keyId: paired.keyId,
        algorithm: 'Ed25519',
        signature: sign(null, decodeBase64Url(sourceChallenge.payload), key.privateKey).toString('base64url'),
      }),
    );
    expect(decodeGatewayAuthenticationServerMessage(targetWire.sent[1]!)).toMatchObject({
      type: 'auth.rejected',
      code: 'challenge_invalid',
    });
  });

  it('rejects a signature submitted after the challenge expires', async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-08-01T09:00:00.000Z'));
      const openprinter = server({ brand: { name: 'Test' }, challengeTtlMs: 5_000 });
      const key = credential();
      const paired = await pairAgent(openprinter, key);
      const wire = transport();
      const session = openprinter.accept({ transport: wire.transport });
      await vi.advanceTimersByTimeAsync(0);
      const challenge = decodeGatewayAuthenticationServerMessage(wire.sent[0]!) as GatewayAuthenticationChallenge;
      vi.setSystemTime(new Date('2026-08-01T09:00:06.000Z'));
      await session.receive(
        encodeGatewayAuthenticationResponse({
          type: 'auth.response',
          challengeId: challenge.challengeId,
          agentId: paired.agentId,
          keyId: paired.keyId,
          algorithm: 'Ed25519',
          signature: sign(null, decodeBase64Url(challenge.payload), key.privateKey).toString('base64url'),
        }),
      );
      expect(decodeGatewayAuthenticationServerMessage(wire.sent[1]!)).toMatchObject({
        type: 'auth.rejected',
        code: 'challenge_expired',
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('times out an unanswered challenge', async () => {
    vi.useFakeTimers();
    try {
      const openprinter = server({ brand: { name: 'Timeout' }, authenticationTimeoutMs: 1_000 });
      const wire = transport();
      const session = openprinter.accept({ transport: wire.transport });
      await vi.advanceTimersByTimeAsync(1_001);
      expect(session.state).toBe('closed');
      expect(decodeGatewayAuthenticationServerMessage(wire.sent[1]!)).toMatchObject({
        type: 'auth.rejected',
        code: 'authentication_timeout',
      });
    } finally {
      vi.useRealTimers();
    }
  });
});
