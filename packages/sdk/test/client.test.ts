import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createOpenPrinterClient,
  isOpenPrinterApiError,
  isOpenPrinterResponseError,
  OpenPrinterConfigurationError,
} from '../src/index.js';

const document = {
  width: 58 as const,
  sections: [{ type: 'text' as const, value: 'OpenPrinter SDK test' }],
};

const job = {
  id: 'job-1',
  projectId: 'project/one',
  printerId: 'printer-1',
  idempotencyKey: 'order-1',
  state: 'PENDING' as const,
  metadata: { source: 'test' },
  expiresAt: '2026-08-21T00:00:00.000Z',
  attemptCount: 0,
  maxAttempts: 5,
  lastErrorCode: null,
  lastErrorMessage: null,
  receivedAt: null,
  submittedAt: null,
  createdAt: '2026-08-21T00:00:00.000Z',
  updatedAt: '2026-08-21T00:00:00.000Z',
};

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('openprinter', () => {
  it('creates a typed job with the project path and bearer key', async () => {
    let receivedBody: unknown;
    let receivedHeaders: Headers | undefined;
    let receivedUrl = '';

    const client = createOpenPrinterClient({
      baseUrl: 'https://api.example.test/',
      apiKey: 'opk_test_secret',
      projectId: 'project/one',
      fetch: (input, init) => {
        receivedUrl = inputUrl(input);
        receivedHeaders = new Headers(init?.headers);
        if (typeof init?.body !== 'string') throw new Error('Expected a JSON request body.');
        receivedBody = JSON.parse(init.body) as unknown;
        return Promise.resolve(jsonResponse({ job, duplicate: false }, { status: 202 }));
      },
      retry: false,
    });

    const response = await client.jobs.create({
      printerId: 'printer-1',
      idempotencyKey: 'order-1',
      document,
      metadata: { source: 'test' },
    });

    expect(response).toEqual({ job, duplicate: false });
    expect(receivedUrl).toBe('https://api.example.test/v1/projects/project%2Fone/jobs');
    expect(receivedHeaders?.get('authorization')).toBe('Bearer opk_test_secret');
    expect(receivedHeaders?.get('content-type')).toBe('application/json');
    expect(receivedBody).toEqual({
      printerId: 'printer-1',
      idempotencyKey: 'order-1',
      document,
      metadata: { source: 'test' },
    });
  });

  it('reads defaults from OPENPRINTER_* environment variables', async () => {
    vi.stubEnv('OPENPRINTER_BASE_URL', 'https://env.example.test');
    vi.stubEnv('OPENPRINTER_API_KEY', 'opk_from_env');
    vi.stubEnv('OPENPRINTER_PROJECT_ID', 'project-from-env');

    const client = createOpenPrinterClient({
      fetch: (input, init) => {
        expect(inputUrl(input)).toBe('https://env.example.test/v1/projects/project-from-env/jobs?state=PENDING');
        expect(new Headers(init?.headers).get('authorization')).toBe('Bearer opk_from_env');
        return Promise.resolve(jsonResponse({ jobs: [job], keyPrefix: 'opk_test' }));
      },
      retry: false,
    });

    await expect(client.jobs.list({ state: 'PENDING' })).resolves.toEqual({
      jobs: [job],
      keyPrefix: 'opk_test',
    });
  });

  it('supports a pinned project view and URL-encodes its identifier', async () => {
    const client = createOpenPrinterClient({
      baseUrl: 'https://api.example.test',
      apiKey: 'opk_test_secret',
      fetch: (input) => {
        expect(inputUrl(input)).toBe('https://api.example.test/v1/projects/project%2Ftwo/jobs');
        return Promise.resolve(jsonResponse({ jobs: [], keyPrefix: 'opk_test' }));
      },
      retry: false,
    });

    await expect(client.project('project/two').jobs.list()).resolves.toEqual({
      jobs: [],
      keyPrefix: 'opk_test',
    });
  });

  it('retries a transient list failure with bounded configuration', async () => {
    let calls = 0;
    const client = createOpenPrinterClient({
      baseUrl: 'https://api.example.test',
      apiKey: 'opk_test_secret',
      projectId: 'project-1',
      fetch: () => {
        calls += 1;
        return Promise.resolve(
          calls === 1
            ? jsonResponse(
                {
                  error: {
                    code: 'temporarily_unavailable',
                    message: 'try again',
                  },
                },
                { status: 503 },
              )
            : jsonResponse({ jobs: [], keyPrefix: 'opk_test' }),
        );
      },
      retry: { maxRetries: 1, baseDelayMs: 0, maxDelayMs: 0 },
    });

    await expect(client.jobs.list()).resolves.toEqual({
      jobs: [],
      keyPrefix: 'opk_test',
    });
    expect(calls).toBe(2);
  });

  it('exposes structured API and response-contract errors', async () => {
    const apiErrorClient = createOpenPrinterClient({
      baseUrl: 'https://api.example.test',
      apiKey: 'opk_test_secret',
      projectId: 'project-1',
      fetch: () =>
        Promise.resolve(
          jsonResponse(
            {
              error: { code: 'forbidden', message: 'Missing jobs:read scope' },
            },
            { status: 403 },
          ),
        ),
      retry: false,
    });

    await expect(apiErrorClient.jobs.list()).rejects.toSatisfy((error: unknown) => {
      return isOpenPrinterApiError(error) && error.status === 403 && error.code === 'forbidden';
    });

    const invalidResponseClient = createOpenPrinterClient({
      baseUrl: 'https://api.example.test',
      apiKey: 'opk_test_secret',
      projectId: 'project-1',
      fetch: () => Promise.resolve(jsonResponse({ jobs: [{ id: 'incomplete' }], keyPrefix: 'opk_test' })),
      retry: false,
    });

    await expect(invalidResponseClient.jobs.list()).rejects.toSatisfy((error: unknown) => {
      return isOpenPrinterResponseError(error);
    });
  });

  it('fails before fetching when an authenticated call has no project', async () => {
    const client = createOpenPrinterClient({
      baseUrl: 'https://api.example.test',
      apiKey: 'opk_test_secret',
      fetch: vi.fn(),
      retry: false,
    });

    await expect(client.jobs.list()).rejects.toBeInstanceOf(OpenPrinterConfigurationError);
  });
});

function inputUrl(input: RequestInfo | URL): string {
  if (typeof input === 'string') return input;
  if (input instanceof URL) return input.toString();
  return input.url;
}

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), {
    headers: { 'content-type': 'application/json' },
    ...init,
  });
}
