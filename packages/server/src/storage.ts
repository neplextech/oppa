import type {
  AgentCredentialRecord,
  AgentCredentialStore,
  CreatePairingCodeStoreInput,
  PairingCodeConsumeResult,
  PairingCodeRecord,
  PairingCodeStore,
} from './types.js';

function normalizedCode(code: string): string {
  return code.replaceAll('-', '').toUpperCase();
}

/** Volatile pairing-code storage for tests and local examples. */
export class InMemoryPairingCodeStore<Metadata> implements PairingCodeStore<Metadata> {
  readonly #records = new Map<string, PairingCodeRecord<Metadata>>();
  readonly #consumed = new Set<string>();
  readonly #consuming = new Set<string>();

  public create(input: CreatePairingCodeStoreInput<Metadata>): Promise<PairingCodeRecord<Metadata>> {
    const key = normalizedCode(input.code);
    if (this.#records.has(key) || this.#consumed.has(key)) {
      throw new Error('The generated pairing code already exists.');
    }
    const record: PairingCodeRecord<Metadata> = {
      ...input,
      createdAt: new Date(input.createdAt),
      expiresAt: new Date(input.expiresAt),
    };
    this.#records.set(key, record);
    return Promise.resolve(structuredClone(record));
  }

  public async consume(
    code: string,
    now: Date,
    register: (record: PairingCodeRecord<Metadata>) => Promise<void>,
  ): Promise<PairingCodeConsumeResult> {
    const key = normalizedCode(code);
    if (this.#consumed.has(key) || this.#consuming.has(key)) {
      return 'already-consumed';
    }
    const record = this.#records.get(key);
    if (record === undefined) {
      return 'invalid';
    }
    if (record.expiresAt.getTime() <= now.getTime()) {
      this.#records.delete(key);
      this.#consumed.add(key);
      return 'expired';
    }

    this.#consuming.add(key);
    try {
      await register(structuredClone(record));
      this.#records.delete(key);
      this.#consumed.add(key);
      return 'consumed';
    } finally {
      this.#consuming.delete(key);
    }
  }
}

/** Volatile public-key storage for tests and local examples. */
export class InMemoryAgentCredentialStore<Metadata> implements AgentCredentialStore<Metadata> {
  readonly #records = new Map<string, AgentCredentialRecord<Metadata>>();

  public create(input: AgentCredentialRecord<Metadata>): Promise<AgentCredentialRecord<Metadata>> {
    const key = this.#key(input.agentId, input.keyId);
    if (this.#records.has(key)) {
      throw new Error('The agent credential already exists.');
    }
    const record = structuredClone(input);
    this.#records.set(key, record);
    return Promise.resolve(structuredClone(record));
  }

  public find(agentId: string, keyId: string): Promise<AgentCredentialRecord<Metadata> | null> {
    const record = this.#records.get(this.#key(agentId, keyId));
    return Promise.resolve(record === undefined ? null : structuredClone(record));
  }

  public revoke(agentId: string, keyId: string, revokedAt: Date): Promise<void> {
    const key = this.#key(agentId, keyId);
    const record = this.#records.get(key);
    if (record !== undefined) {
      this.#records.set(key, { ...record, revokedAt: new Date(revokedAt) });
    }
    return Promise.resolve();
  }

  #key(agentId: string, keyId: string): string {
    return `${agentId}\u0000${keyId}`;
  }
}
