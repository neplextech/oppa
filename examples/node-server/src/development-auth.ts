import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';

const DEFAULT_CLIENT_ID = 'oppa-desktop';
const AUTHORIZATION_CODE_TTL_MS = 2 * 60 * 1_000;
const ACCESS_TOKEN_TTL_MS = 60 * 60 * 1_000;
const MAX_ACTIVE_CREDENTIALS = 256;
const PKCE_VALUE_PATTERN = /^[A-Za-z0-9\-._~]{43,128}$/;
const PKCE_CHALLENGE_PATTERN = /^[A-Za-z0-9_-]{43}$/;

interface AuthorizationCode {
  readonly agentId: string;
  readonly clientId: string;
  readonly codeChallenge: string;
  readonly expiresAt: number;
  readonly redirectUri: string;
}

interface AccessToken {
  readonly agentId: string;
  readonly expiresAt: number;
  readonly issuedAt: number;
}

/** A validated development authorization request. */
export interface DevelopmentAuthorizationRequest {
  readonly clientId: string;
  readonly codeChallenge: string;
  readonly redirectUri: string;
  readonly state: string;
}

/** The non-secret identity associated with an in-memory access token. */
export interface DevelopmentAgentIdentity {
  readonly agentId: string;
  readonly expiresAt: string;
  readonly issuedAt: string;
}

/** The successful result of exchanging a one-time authorization code. */
export interface DevelopmentTokenResponse {
  readonly accessToken: string;
  readonly agentId: string;
  readonly expiresIn: number;
  readonly tokenType: 'Bearer';
}

/**
 * A safe OAuth-style error for the local development authorization endpoints.
 * Its message never contains codes, tokens, challenges, or verifiers.
 */
export class DevelopmentAuthError extends Error {
  public readonly error:
    | 'invalid_request'
    | 'invalid_client'
    | 'invalid_grant'
    | 'unsupported_grant_type'
    | 'unsupported_response_type';

  public constructor(error: DevelopmentAuthError['error'], message: string) {
    super(message);
    this.name = 'DevelopmentAuthError';
    this.error = error;
  }
}

/**
 * Minimal, bounded, in-memory PKCE credential store for the loopback example.
 *
 * This is intentionally not a production identity provider. Restarting the
 * process revokes every credential.
 */
export class DevelopmentAuthStore {
  readonly #accessTokens = new Map<string, AccessToken>();
  readonly #authorizationCodes = new Map<string, AuthorizationCode>();
  readonly #clientId: string;
  readonly #now: () => number;

  public constructor(options?: { readonly clientId?: string; readonly now?: () => number }) {
    this.#clientId = options?.clientId ?? DEFAULT_CLIENT_ID;
    this.#now = options?.now ?? Date.now;
  }

  /** Parse and validate an OAuth-style authorization request. */
  public parseAuthorizationRequest(url: URL): DevelopmentAuthorizationRequest {
    const responseType = url.searchParams.get('response_type');
    const clientId = url.searchParams.get('client_id');
    const redirectUriValue = url.searchParams.get('redirect_uri');
    const state = url.searchParams.get('state');
    const codeChallenge = url.searchParams.get('code_challenge');
    const codeChallengeMethod = url.searchParams.get('code_challenge_method');

    if (responseType !== 'code') {
      throw new DevelopmentAuthError(
        'unsupported_response_type',
        'Only the authorization-code response type is supported.',
      );
    }

    if (clientId !== this.#clientId) {
      throw new DevelopmentAuthError('invalid_client', 'The development client ID is invalid.');
    }

    if (state === null || state.length < 16 || state.length > 512) {
      throw new DevelopmentAuthError('invalid_request', 'A state value between 16 and 512 characters is required.');
    }

    if (codeChallengeMethod !== 'S256' || codeChallenge === null || !PKCE_CHALLENGE_PATTERN.test(codeChallenge)) {
      throw new DevelopmentAuthError('invalid_request', 'A valid S256 PKCE challenge is required.');
    }

    if (redirectUriValue === null) {
      throw new DevelopmentAuthError('invalid_request', 'A loopback redirect URI is required.');
    }

    const redirectUri = parseLoopbackRedirectUri(redirectUriValue);

    return {
      clientId,
      codeChallenge,
      redirectUri: redirectUri.toString(),
      state,
    };
  }

  /**
   * Issue a short-lived one-time code and return the OPPA loopback redirect.
   */
  public approveAuthorization(request: DevelopmentAuthorizationRequest): URL {
    this.#prune();

    const code = randomSecret();
    const agentId = `agent_${randomBytes(12).toString('base64url')}`;
    putBounded(
      this.#authorizationCodes,
      code,
      {
        agentId,
        clientId: request.clientId,
        codeChallenge: request.codeChallenge,
        expiresAt: this.#now() + AUTHORIZATION_CODE_TTL_MS,
        redirectUri: request.redirectUri,
      },
      MAX_ACTIVE_CREDENTIALS,
    );

    const redirect = new URL(request.redirectUri);
    redirect.searchParams.set('code', code);
    redirect.searchParams.set('state', request.state);
    return redirect;
  }

  /**
   * Consume an authorization code and issue an opaque, in-memory Bearer token.
   */
  public exchangeAuthorizationCode(parameters: URLSearchParams): DevelopmentTokenResponse {
    this.#prune();

    if (parameters.get('grant_type') !== 'authorization_code') {
      throw new DevelopmentAuthError('unsupported_grant_type', 'Only the authorization_code grant is supported.');
    }

    const code = parameters.get('code');
    const clientId = parameters.get('client_id');
    const redirectUri = parameters.get('redirect_uri');
    const verifier = parameters.get('code_verifier');

    if (code === null || clientId === null || redirectUri === null || verifier === null) {
      throw new DevelopmentAuthError(
        'invalid_request',
        'The code, client ID, redirect URI, and PKCE verifier are required.',
      );
    }

    const record = this.#authorizationCodes.get(code);
    // A code is consumed by its first exchange attempt, including a failed
    // verifier check, so it cannot become a reusable guessing oracle.
    this.#authorizationCodes.delete(code);

    if (
      record === undefined ||
      record.expiresAt <= this.#now() ||
      record.clientId !== clientId ||
      record.redirectUri !== parseLoopbackRedirectUri(redirectUri).toString() ||
      !verifyPkce(record.codeChallenge, verifier)
    ) {
      throw new DevelopmentAuthError(
        'invalid_grant',
        'The authorization code is invalid, expired, or does not match PKCE.',
      );
    }

    const issuedAt = this.#now();
    const expiresAt = issuedAt + ACCESS_TOKEN_TTL_MS;
    const accessToken = randomSecret();
    putBounded(
      this.#accessTokens,
      accessToken,
      {
        agentId: record.agentId,
        expiresAt,
        issuedAt,
      },
      MAX_ACTIVE_CREDENTIALS,
    );

    return {
      accessToken,
      agentId: record.agentId,
      expiresIn: Math.floor(ACCESS_TOKEN_TTL_MS / 1_000),
      tokenType: 'Bearer',
    };
  }

  /**
   * Resolve an opaque token without revealing it to logs or public state.
   */
  public authenticateAccessToken(token: string): DevelopmentAgentIdentity | null {
    const record = this.#accessTokens.get(token);

    if (record === undefined) {
      return null;
    }

    if (record.expiresAt <= this.#now()) {
      this.#accessTokens.delete(token);
      return null;
    }

    return {
      agentId: record.agentId,
      expiresAt: new Date(record.expiresAt).toISOString(),
      issuedAt: new Date(record.issuedAt).toISOString(),
    };
  }

  /** Revoke all development credentials. */
  public revokeAll(): void {
    this.#accessTokens.clear();
    this.#authorizationCodes.clear();
  }

  #prune(): void {
    const now = this.#now();

    for (const [code, record] of this.#authorizationCodes) {
      if (record.expiresAt <= now) {
        this.#authorizationCodes.delete(code);
      }
    }

    for (const [token, record] of this.#accessTokens) {
      if (record.expiresAt <= now) {
        this.#accessTokens.delete(token);
      }
    }
  }
}

function parseLoopbackRedirectUri(value: string): URL {
  let url: URL;

  try {
    url = new URL(value);
  } catch {
    throw new DevelopmentAuthError('invalid_request', 'The redirect URI is invalid.');
  }

  if (
    url.protocol !== 'http:' ||
    url.hostname !== '127.0.0.1' ||
    url.port === '' ||
    url.username !== '' ||
    url.password !== '' ||
    url.hash !== ''
  ) {
    throw new DevelopmentAuthError('invalid_request', 'The redirect URI must use an explicit port on 127.0.0.1.');
  }

  return url;
}

function verifyPkce(expectedChallenge: string, verifier: string): boolean {
  if (!PKCE_VALUE_PATTERN.test(verifier)) {
    return false;
  }

  const actualChallenge = createHash('sha256').update(verifier, 'ascii').digest('base64url');
  const expected = Buffer.from(expectedChallenge, 'ascii');
  const actual = Buffer.from(actualChallenge, 'ascii');

  return expected.byteLength === actual.byteLength && timingSafeEqual(expected, actual);
}

function randomSecret(): string {
  return randomBytes(32).toString('base64url');
}

function putBounded<Key, Value>(map: Map<Key, Value>, key: Key, value: Value, maximumSize: number): void {
  while (map.size >= maximumSize) {
    const oldest = map.keys().next();

    if (oldest.done) {
      break;
    }

    map.delete(oldest.value);
  }

  map.set(key, value);
}
