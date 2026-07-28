import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  AGENT_MESSAGE_TYPES,
  MAX_WIRE_MESSAGE_BYTES,
  ProtocolError,
  ProtocolValidationError,
  SERVER_MESSAGE_TYPES,
  UnsupportedProtocolVersionError,
  decodeAgentMessage,
  decodeProtocolMessage,
  decodeServerMessage,
  encodeAgentMessage,
  encodeServerMessage,
  parsePrintDocument,
  parsePrintJob,
  parsePrinterDescriptor,
  parseProtocolMessage,
} from '../src/index.js';

const fixtureRoot = fileURLToPath(new URL('../../../protocol/fixtures', import.meta.url));

function fixtureFiles(group: 'agent' | 'server' | 'invalid'): string[] {
  return readdirSync(`${fixtureRoot}/${group}`)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => `${fixtureRoot}/${group}/${name}`);
}

function expectProtocolError(operation: () => unknown, code: ProtocolError['code']): void {
  let thrown: unknown;
  try {
    operation();
  } catch (cause) {
    thrown = cause;
  }

  expect(thrown).toBeInstanceOf(ProtocolError);
  if (!(thrown instanceof ProtocolError)) {
    throw new Error('Expected a ProtocolError');
  }
  expect(thrown.code).toBe(code);
}

describe('shared protocol fixtures', () => {
  const observedAgentTypes = new Set<string>();
  const observedServerTypes = new Set<string>();

  for (const path of fixtureFiles('agent')) {
    it(`accepts and round-trips ${path.split('/').at(-1)}`, () => {
      const raw = readFileSync(path, 'utf8');
      const message = decodeAgentMessage(raw);
      observedAgentTypes.add(message.type);

      expect(JSON.parse(encodeAgentMessage(message))).toEqual(JSON.parse(raw));
      expect(decodeProtocolMessage(new TextEncoder().encode(raw))).toEqual(message);
    });
  }

  for (const path of fixtureFiles('server')) {
    it(`accepts and round-trips ${path.split('/').at(-1)}`, () => {
      const raw = readFileSync(path, 'utf8');
      const message = decodeServerMessage(raw);
      observedServerTypes.add(message.type);

      expect(JSON.parse(encodeServerMessage(message))).toEqual(JSON.parse(raw));
      expect(decodeProtocolMessage(new TextEncoder().encode(raw))).toEqual(message);
    });
  }

  for (const path of fixtureFiles('invalid')) {
    it(`rejects ${path.split('/').at(-1)}`, () => {
      const raw = readFileSync(path, 'utf8');
      if (path.endsWith('unsupported-version.json')) {
        expect(() => decodeProtocolMessage(raw)).toThrow(UnsupportedProtocolVersionError);
      } else {
        expect(() => decodeProtocolMessage(raw)).toThrow(ProtocolValidationError);
      }
    });
  }

  it('covers every documented message discriminator', () => {
    // The fixture tests above are declared first and execute in declaration
    // order under Vitest's default sequence.
    expect([...observedAgentTypes].sort()).toEqual([...AGENT_MESSAGE_TYPES].sort());
    expect([...observedServerTypes].sort()).toEqual([...SERVER_MESSAGE_TYPES].sort());
  });
});

describe('runtime validation', () => {
  it('validates standalone HTTP print-job bodies', () => {
    const envelope = JSON.parse(readFileSync(`${fixtureRoot}/server/print-job.json`, 'utf8')) as {
      payload: unknown;
    };

    expect(parsePrintJob(envelope.payload)).toEqual(envelope.payload);
  });

  it('applies string bounds in JavaScript UTF-16 code units', () => {
    const envelope = JSON.parse(readFileSync(`${fixtureRoot}/server/print-job.json`, 'utf8')) as {
      payload: Record<string, unknown>;
    };
    const atLimit = '😀'.repeat(128);
    const overLimit = `${atLimit}a`;

    expect(atLimit.length).toBe(256);
    expect(parsePrintJob({ ...envelope.payload, idempotencyKey: atLimit })).toMatchObject({
      idempotencyKey: atLimit,
    });
    expect(() => parsePrintJob({ ...envelope.payload, idempotencyKey: overLimit })).toThrow(ProtocolValidationError);
  });

  it('allows printer capabilities to be omitted when discovery cannot determine them', () => {
    const envelope = JSON.parse(
      readFileSync(`${fixtureRoot}/agent/printer-inventory-unknown-capabilities.json`, 'utf8'),
    ) as {
      payload: {
        printers: Array<Record<string, unknown>>;
      };
    };
    const descriptor = envelope.payload.printers[0]!;

    expect(parsePrinterDescriptor(descriptor)).toEqual(descriptor);
    expect(descriptor).not.toHaveProperty('capabilities');
  });

  it('rejects unknown fields and the non-protocol printed status', () => {
    const fixture = JSON.parse(readFileSync(`${fixtureRoot}/agent/job-submitted.json`, 'utf8')) as Record<
      string,
      unknown
    > & {
      payload: Record<string, unknown>;
    };

    expect(() => parseProtocolMessage({ ...fixture, unexpected: true })).toThrow(ProtocolValidationError);
    expect(() =>
      parseProtocolMessage({
        ...fixture,
        payload: { ...fixture.payload, status: 'printed' },
      }),
    ).toThrow(ProtocolValidationError);
  });

  it('rejects malformed JSON and oversized UTF-8 messages', () => {
    expectProtocolError(() => decodeProtocolMessage('{'), 'invalid_json');
    expectProtocolError(() => decodeProtocolMessage('x'.repeat(MAX_WIRE_MESSAGE_BYTES + 1)), 'message_too_large');
  });

  it('rejects local paths and malformed base64 image data', () => {
    expect(() =>
      parsePrintDocument({
        width: 80,
        sections: [
          {
            type: 'image',
            mediaType: 'image/png',
            data: '/tmp/receipt.png',
          },
        ],
      }),
    ).toThrow(ProtocolValidationError);
  });

  it('rejects metadata keys outside the bounded opaque namespace', () => {
    const envelope = JSON.parse(readFileSync(`${fixtureRoot}/server/print-job.json`, 'utf8')) as {
      payload: Record<string, unknown>;
    };

    expect(() =>
      parsePrintJob({
        ...envelope.payload,
        metadata: { 'invalid key': 'value' },
      }),
    ).toThrow(ProtocolValidationError);
  });

  it('uses payload-free expected errors', () => {
    try {
      decodeProtocolMessage(readFileSync(`${fixtureRoot}/invalid/unsupported-version.json`, 'utf8'));
      throw new Error('expected version rejection');
    } catch (error) {
      expect(error).toBeInstanceOf(ProtocolError);
      expect(error).toMatchObject({
        code: 'unsupported_protocol_version',
        receivedVersion: 99,
        supportedVersions: [1],
      });
      expect(JSON.stringify(error)).not.toContain('server.heartbeat');
    }
  });
});
