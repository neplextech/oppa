import { createHash } from 'node:crypto';

import { describe, expect, it } from 'vitest';

import { DevelopmentAuthError, DevelopmentAuthStore } from '../src/development-auth.js';
import { EXAMPLE_CLIENT_ID, loadExampleClientId } from '../src/product-config.js';

const verifier = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.';
const challenge = createHash('sha256').update(verifier, 'ascii').digest('base64url');

describe('DevelopmentAuthStore', () => {
  it('uses the OAuth client registered by the bundled OPPA product', () => {
    expect(EXAMPLE_CLIENT_ID).toBe('oppa-desktop');
    expect(loadExampleClientId()).toBe(EXAMPLE_CLIENT_ID);
    expect(() =>
      new DevelopmentAuthStore({ clientId: EXAMPLE_CLIENT_ID }).parseAuthorizationRequest(authorizationUrl()),
    ).not.toThrow();
  });

  it('binds a one-time PKCE code and token to one agent identity', () => {
    const store = new DevelopmentAuthStore();
    const authorization = store.parseAuthorizationRequest(authorizationUrl());
    const redirect = store.approveAuthorization(authorization);
    const code = redirect.searchParams.get('code');

    expect(code).not.toBeNull();
    expect(redirect.searchParams.get('state')).toBe('0123456789abcdef');

    const token = store.exchangeAuthorizationCode(tokenParameters(code ?? ''));
    const identity = store.authenticateAccessToken(token.accessToken);

    expect(identity?.agentId).toBe(token.agentId);
    expect(token.tokenType).toBe('Bearer');
    expect(token.expiresIn).toBe(3_600);
    expect(() => store.exchangeAuthorizationCode(tokenParameters(code ?? ''))).toThrowError(DevelopmentAuthError);
  });

  it('rejects non-loopback callbacks before issuing a code', () => {
    const store = new DevelopmentAuthStore();
    const url = authorizationUrl();
    url.searchParams.set('redirect_uri', 'https://attacker.example/callback');

    expect(() => store.parseAuthorizationRequest(url)).toThrowError(/127\.0\.0\.1/);
  });

  it('expires authorization codes and access tokens', () => {
    let now = Date.parse('2026-07-28T10:00:00.000Z');
    const store = new DevelopmentAuthStore({
      now: () => now,
    });
    const redirect = store.approveAuthorization(store.parseAuthorizationRequest(authorizationUrl()));
    const code = redirect.searchParams.get('code') ?? '';

    now += 2 * 60 * 1_000 + 1;
    expect(() => store.exchangeAuthorizationCode(tokenParameters(code))).toThrowError(
      /invalid, expired, or does not match PKCE/,
    );

    const secondRedirect = store.approveAuthorization(store.parseAuthorizationRequest(authorizationUrl()));
    const token = store.exchangeAuthorizationCode(tokenParameters(secondRedirect.searchParams.get('code') ?? ''));
    now += 60 * 60 * 1_000 + 1;

    expect(store.authenticateAccessToken(token.accessToken)).toBeNull();
  });

  it('consumes a code after an invalid verifier attempt', () => {
    const store = new DevelopmentAuthStore();
    const redirect = store.approveAuthorization(store.parseAuthorizationRequest(authorizationUrl()));
    const code = redirect.searchParams.get('code') ?? '';
    const invalid = tokenParameters(code);
    invalid.set('code_verifier', 'z'.repeat(verifier.length));

    expect(() => store.exchangeAuthorizationCode(invalid)).toThrowError(DevelopmentAuthError);
    expect(() => store.exchangeAuthorizationCode(tokenParameters(code))).toThrowError(DevelopmentAuthError);
  });
});

function authorizationUrl(): URL {
  const url = new URL('http://127.0.0.1:8787/authorize');
  url.searchParams.set('response_type', 'code');
  url.searchParams.set('client_id', EXAMPLE_CLIENT_ID);
  url.searchParams.set('redirect_uri', 'http://127.0.0.1:49152/callback');
  url.searchParams.set('state', '0123456789abcdef');
  url.searchParams.set('code_challenge', challenge);
  url.searchParams.set('code_challenge_method', 'S256');
  return url;
}

function tokenParameters(code: string): URLSearchParams {
  return new URLSearchParams({
    grant_type: 'authorization_code',
    client_id: EXAMPLE_CLIENT_ID,
    redirect_uri: 'http://127.0.0.1:49152/callback',
    code,
    code_verifier: verifier,
  });
}
