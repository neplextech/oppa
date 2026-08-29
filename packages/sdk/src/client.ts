import {
  OPENPRINTER_DISCOVERY_PATH,
  OPENPRINTER_PAIRING_PATH,
  parseDiscoveryDocument,
  parsePairingRequest,
  parsePairingResponse,
  parsePrintDocument,
  type OpenPrinterDiscoveryDocument,
  type OpenPrinterPairingRequest,
  type OpenPrinterPairingResponse,
} from '@openprinter/protocol';

import {
  OpenPrinterApiError,
  OpenPrinterConfigurationError,
  OpenPrinterResponseError,
  OpenPrinterTimeoutError,
} from './errors.js';
import {
  OPENPRINTER_PRINT_JOB_STATES,
  type CreatePrintJobInput,
  type CreatePrintJobResponse,
  type ListPrintJobsOptions,
  type ListPrintJobsResponse,
  type OpenPrinterApiKey,
  type OpenPrinterClient,
  type OpenPrinterClientOptions,
  type OpenPrinterHealth,
  type OpenPrinterJob,
  type OpenPrinterJobsResource,
  type OpenPrinterProjectClient,
  type OpenPrinterRetryOptions,
} from './types.js';

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_RETRIES = 2;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_MAX_DELAY_MS = 5_000;
const MAX_METADATA_ENTRIES = 32;
const MAX_METADATA_KEY_LENGTH = 64;
const MAX_METADATA_VALUE_LENGTH = 1_000;
const MAX_JOB_EXPIRY_MS = 30 * 24 * 60 * 60_000;
const RETRYABLE_STATUSES = new Set([408, 425, 429, 500, 502, 503, 504]);
const RETRYABLE_METHODS = new Set(['GET', 'HEAD']);

type RequestMethod = 'DELETE' | 'GET' | 'PATCH' | 'POST' | 'PUT';

interface InternalRequestOptions {
  readonly method: RequestMethod;
  readonly body: unknown;
  readonly authenticated: boolean;
  readonly signal: AbortSignal | undefined;
  readonly retryable: boolean;
}

interface ResolvedRetryOptions {
  readonly maxRetries: number;
  readonly baseDelayMs: number;
  readonly maxDelayMs: number;
}

interface RequestSignal {
  readonly signal: AbortSignal;
  readonly timedOut: () => boolean;
  dispose(): void;
}

interface ParsedApiError {
  readonly code: string | undefined;
  readonly message: string | undefined;
  readonly requestId: string | undefined;
  readonly details: unknown;
}

/**
 * Create a client for the public Neplex OpenPrinter Cloud API.
 *
 * The constructor reads omitted values from `process.env` in Node-compatible
 * runtimes. It does not require an API key until an authenticated method is
 * called, so discovery and health checks can be used without credentials.
 */
export function createOpenPrinterClient(options: OpenPrinterClientOptions = {}): OpenPrinterClient {
  const baseUrl = resolveBaseUrl(options.baseUrl);
  const apiKey = resolveApiKeyOption(options.apiKey);
  const projectId = optionalIdentifier(options.projectId ?? readEnvironment('OPENPRINTER_PROJECT_ID'), 'projectId');
  const timeoutMs = resolveIntegerOption(
    options.timeoutMs ?? parseEnvironmentInteger('OPENPRINTER_TIMEOUT_MS'),
    DEFAULT_TIMEOUT_MS,
    'timeoutMs',
    1,
    86_400_000,
  );
  const retry = resolveRetryOptions(options.retry, parseEnvironmentInteger('OPENPRINTER_MAX_RETRIES'));
  const requestImpl = options.fetch ?? globalThis.fetch;
  if (typeof requestImpl !== 'function') {
    throw new OpenPrinterConfigurationError('A Fetch API implementation is required to use @openprinter/sdk.');
  }

  const request = createRequestFunction({
    baseUrl,
    apiKey,
    timeoutMs,
    retry,
    fetch: requestImpl,
    headers: options.headers,
  });
  const jobs = createJobsResource(undefined, request, projectId);

  const client: OpenPrinterClient = {
    baseUrl: baseUrl.toString().replace(/\/$/, ''),
    projectId,
    jobs,
    project(nextProjectId: string): OpenPrinterProjectClient {
      const resolvedProjectId = requiredIdentifier(nextProjectId, 'projectId');
      return Object.freeze({
        projectId: resolvedProjectId,
        jobs: createJobsResource(resolvedProjectId, request, projectId),
      });
    },
    health(optionsForRequest = {}): Promise<OpenPrinterHealth> {
      return request('health', parseHealth, {
        method: 'GET',
        body: undefined,
        authenticated: false,
        signal: optionsForRequest.signal,
        retryable: true,
      });
    },
    discovery(optionsForRequest = {}): Promise<OpenPrinterDiscoveryDocument> {
      return request(OPENPRINTER_DISCOVERY_PATH, parseDiscovery, {
        method: 'GET',
        body: undefined,
        authenticated: false,
        signal: optionsForRequest.signal,
        retryable: true,
      });
    },
    async pair(input: OpenPrinterPairingRequest, optionsForRequest = {}): Promise<OpenPrinterPairingResponse> {
      const pairingRequest = parsePairingRequest(input);
      return request(OPENPRINTER_PAIRING_PATH, parsePairing, {
        method: 'POST',
        body: pairingRequest,
        authenticated: false,
        signal: optionsForRequest.signal,
        retryable: false,
      });
    },
  };

  return Object.freeze(client);
}

function createJobsResource(
  pinnedProjectId: string | undefined,
  request: RequestFunction,
  defaultProjectId: string | undefined,
): OpenPrinterJobsResource {
  const resource: OpenPrinterJobsResource = {
    async create(input: CreatePrintJobInput, optionsForRequest = {}): Promise<CreatePrintJobResponse> {
      const projectId = pinnedProjectId ?? defaultProjectId;
      if (projectId === undefined) {
        throw new OpenPrinterConfigurationError(
          'A projectId is required for jobs. Pass it to createOpenPrinterClient or call client.project(projectId).',
        );
      }
      const body = validateCreatePrintJobInput(input);
      return request(`v1/projects/${encodeSegment(projectId)}/jobs`, parseCreatePrintJobResponse, {
        method: 'POST',
        body,
        authenticated: true,
        signal: optionsForRequest.signal,
        retryable: true,
      });
    },
    async list(optionsForRequest: ListPrintJobsOptions = {}): Promise<ListPrintJobsResponse> {
      const projectId = pinnedProjectId ?? defaultProjectId;
      if (projectId === undefined) {
        throw new OpenPrinterConfigurationError(
          'A projectId is required for jobs. Pass it to createOpenPrinterClient or call client.project(projectId).',
        );
      }
      const query = new URLSearchParams();
      if (optionsForRequest.state !== undefined) query.set('state', optionsForRequest.state);
      const suffix = query.size === 0 ? '' : `?${query.toString()}`;
      return request(`v1/projects/${encodeSegment(projectId)}/jobs${suffix}`, parseListPrintJobsResponse, {
        method: 'GET',
        body: undefined,
        authenticated: true,
        signal: optionsForRequest.signal,
        retryable: true,
      });
    },
  };
  return Object.freeze(resource);
}

type RequestFunction = <T>(endpoint: string, parser: ResponseParser<T>, options: InternalRequestOptions) => Promise<T>;
type ResponseParser<T> = (value: unknown, method: string, url: string) => T;

function createRequestFunction(options: {
  readonly baseUrl: URL;
  readonly apiKey: OpenPrinterApiKey | undefined;
  readonly timeoutMs: number;
  readonly retry: ResolvedRetryOptions;
  readonly fetch: typeof globalThis.fetch;
  readonly headers: HeadersInit | undefined;
}): RequestFunction {
  return async function request<T>(
    endpoint: string,
    parser: ResponseParser<T>,
    requestOptions: InternalRequestOptions,
  ) {
    const url = resolveEndpoint(options.baseUrl, endpoint);
    const headers = new Headers(options.headers);
    headers.set('accept', 'application/json');
    if (requestOptions.body !== undefined) headers.set('content-type', 'application/json');
    if (requestOptions.authenticated) {
      const secret = await readApiKey(options.apiKey);
      headers.set('authorization', `Bearer ${secret}`);
    }

    const method = requestOptions.method;
    const canRetry = requestOptions.retryable && (RETRYABLE_METHODS.has(method) || method === 'POST');
    const maxRetries = canRetry ? options.retry.maxRetries : 0;

    for (let retryIndex = 0; ; retryIndex += 1) {
      const requestSignal = createRequestSignal(requestOptions.signal, options.timeoutMs);
      try {
        const response = await options.fetch(url, {
          method,
          headers,
          ...(requestOptions.body === undefined ? {} : { body: JSON.stringify(requestOptions.body) }),
          cache: 'no-store',
          credentials: 'omit',
          redirect: 'error',
          signal: requestSignal.signal,
        });

        if (requestSignal.timedOut()) {
          throw new OpenPrinterTimeoutError(method, url.toString(), options.timeoutMs);
        }

        const responseBody = await readJson(response);
        if (!response.ok) {
          const apiError = createApiError(response, method, url, responseBody);
          if (retryIndex < maxRetries && RETRYABLE_STATUSES.has(response.status)) {
            await waitForRetry(retryDelay(options.retry, retryIndex, response), requestOptions.signal);
            continue;
          }
          throw apiError;
        }

        return parser(responseBody, method, url.toString());
      } catch (error) {
        if (error instanceof OpenPrinterApiError || error instanceof OpenPrinterResponseError) throw error;
        if (requestSignal.timedOut()) {
          const timeoutError = new OpenPrinterTimeoutError(method, url.toString(), options.timeoutMs);
          if (retryIndex < maxRetries) {
            await waitForRetry(retryDelay(options.retry, retryIndex), requestOptions.signal);
            continue;
          }
          throw timeoutError;
        }
        if (error instanceof OpenPrinterTimeoutError) {
          if (retryIndex < maxRetries) {
            await waitForRetry(retryDelay(options.retry, retryIndex), requestOptions.signal);
            continue;
          }
          throw error;
        }
        if (requestOptions.signal?.aborted) throw error;
        if (retryIndex < maxRetries) {
          await waitForRetry(retryDelay(options.retry, retryIndex), requestOptions.signal);
          continue;
        }
        throw error;
      } finally {
        requestSignal.dispose();
      }
    }
  };
}

function resolveBaseUrl(value: string | undefined): URL {
  const raw =
    value ??
    readEnvironment('OPENPRINTER_BASE_URL') ??
    readEnvironment('OPENPRINTER_API_BASE_URL') ??
    readEnvironment('OPENPRINTER_PUBLIC_BASE_URL');
  if (raw === undefined || raw.trim() === '') {
    throw new OpenPrinterConfigurationError(
      'An OpenPrinter base URL is required. Set OPENPRINTER_BASE_URL, OPENPRINTER_API_BASE_URL, or OPENPRINTER_PUBLIC_BASE_URL.',
    );
  }
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch (error) {
    throw new OpenPrinterConfigurationError(`The OpenPrinter base URL is invalid: ${errorMessage(error)}.`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new OpenPrinterConfigurationError('The OpenPrinter base URL must use HTTP or HTTPS.');
  }
  if (parsed.search !== '' || parsed.hash !== '') {
    throw new OpenPrinterConfigurationError('The OpenPrinter base URL must not include a query string or fragment.');
  }
  parsed.pathname = `${parsed.pathname.replace(/\/+$/, '')}/`;
  return parsed;
}

function resolveEndpoint(baseUrl: URL, endpoint: string): URL {
  return new URL(endpoint.replace(/^\/+/, ''), baseUrl);
}

function encodeSegment(value: string): string {
  return encodeURIComponent(value);
}

function resolveApiKeyOption(value: OpenPrinterApiKey | undefined): OpenPrinterApiKey | undefined {
  return value ?? readEnvironment('OPENPRINTER_API_KEY');
}

async function readApiKey(value: OpenPrinterApiKey | undefined): Promise<string> {
  if (value === undefined) {
    throw new OpenPrinterConfigurationError(
      'An OpenPrinter API key is required for jobs. Set OPENPRINTER_API_KEY or pass apiKey to the client.',
    );
  }
  const secret = typeof value === 'function' ? await value() : value;
  if (secret.trim() === '') throw new OpenPrinterConfigurationError('The OpenPrinter API key must not be empty.');
  return secret.trim();
}

function requiredIdentifier(value: string, name: string): string {
  if (typeof value !== 'string' || value.trim() === '' || value.length > 256) {
    throw new OpenPrinterConfigurationError(`${name} must contain between 1 and 256 characters.`);
  }
  return value;
}

function optionalIdentifier(value: string | undefined, name: string): string | undefined {
  return value === undefined || value.trim() === '' ? undefined : requiredIdentifier(value, name);
}

function resolveIntegerOption(
  value: number | undefined,
  fallback: number,
  name: string,
  minimum: number,
  maximum: number,
): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved < minimum || resolved > maximum) {
    throw new OpenPrinterConfigurationError(`${name} must be an integer between ${minimum} and ${maximum}.`);
  }
  return resolved;
}

function resolveRetryOptions(
  value: false | OpenPrinterRetryOptions | undefined,
  environmentMaxRetries: number | undefined,
): ResolvedRetryOptions {
  if (value === false) return { maxRetries: 0, baseDelayMs: 0, maxDelayMs: 0 };
  const maxRetries = resolveIntegerOption(
    value?.maxRetries ?? environmentMaxRetries,
    DEFAULT_MAX_RETRIES,
    'maxRetries',
    0,
    10,
  );
  const baseDelayMs = resolveIntegerOption(value?.baseDelayMs, DEFAULT_BASE_DELAY_MS, 'baseDelayMs', 0, 60_000);
  const maxDelayMs = resolveIntegerOption(value?.maxDelayMs, DEFAULT_MAX_DELAY_MS, 'maxDelayMs', baseDelayMs, 120_000);
  return { maxRetries, baseDelayMs, maxDelayMs };
}

function parseEnvironmentInteger(name: string): number | undefined {
  const raw = readEnvironment(name);
  if (raw === undefined || raw.trim() === '') return undefined;
  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed)) {
    throw new OpenPrinterConfigurationError(`${name} must be a safe integer.`);
  }
  return parsed;
}

function readEnvironment(name: string): string | undefined {
  return typeof process === 'undefined' ? undefined : process.env[name];
}

function createRequestSignal(parent: AbortSignal | undefined, timeoutMs: number): RequestSignal {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);
  const abort = () => controller.abort(parent?.reason);
  if (parent?.aborted) controller.abort(parent.reason);
  else parent?.addEventListener('abort', abort, { once: true });
  return {
    signal: controller.signal,
    timedOut: () => timedOut,
    dispose() {
      clearTimeout(timer);
      parent?.removeEventListener('abort', abort);
    },
  };
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text.trim() === '') return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function createApiError(response: Response, method: string, url: URL, body: unknown): OpenPrinterApiError {
  const parsed = parseApiError(body);
  const options: {
    status: number;
    method: string;
    url: string;
    code?: string;
    message?: string;
    requestId?: string;
    details?: unknown;
  } = {
    status: response.status,
    method,
    url: url.toString(),
  };
  if (parsed.code !== undefined) options.code = parsed.code;
  if (parsed.message !== undefined) options.message = parsed.message;
  const requestId = parsed.requestId ?? response.headers.get('x-request-id') ?? undefined;
  if (requestId !== undefined) options.requestId = requestId;
  if (parsed.details !== undefined) options.details = parsed.details;
  return new OpenPrinterApiError(options);
}

function parseApiError(value: unknown): ParsedApiError {
  if (!isRecord(value) || !isRecord(value.error)) {
    return { code: undefined, message: undefined, requestId: undefined, details: undefined };
  }
  const error = value.error;
  return {
    code: stringOrUndefined(error.code),
    message: stringOrUndefined(error.message),
    requestId: stringOrUndefined(error.requestId) ?? stringOrUndefined(value.requestId),
    details: error,
  };
}

function retryDelay(options: ResolvedRetryOptions, retryIndex: number, response?: Response): number {
  const retryAfter = response === undefined ? undefined : parseRetryAfter(response.headers.get('retry-after'));
  if (retryAfter !== undefined) return Math.min(retryAfter, options.maxDelayMs);
  return Math.min(options.baseDelayMs * 2 ** retryIndex, options.maxDelayMs);
}

function parseRetryAfter(value: string | null): number | undefined {
  if (value === null || value.trim() === '') return undefined;
  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds >= 0) return Math.round(seconds * 1_000);
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? undefined : Math.max(0, timestamp - Date.now());
}

function waitForRetry(delayMs: number, signal: AbortSignal | undefined): Promise<void> {
  if (delayMs === 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    function abort() {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
      const reason: unknown = signal?.reason;
      reject(reason instanceof Error ? reason : new DOMException('The request was aborted.', 'AbortError'));
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', abort);
      resolve();
    }, delayMs);
    if (signal?.aborted) abort();
    else signal?.addEventListener('abort', abort, { once: true });
  });
}

function validateCreatePrintJobInput(input: CreatePrintJobInput): CreatePrintJobInput {
  if (!isRecord(input)) throw new TypeError('The print-job input must be an object.');
  requiredInputString(input.printerId, 'printerId', 256);
  requiredInputString(input.idempotencyKey, 'idempotencyKey', 256);
  const document = parsePrintDocument(input.document);
  validateMetadata(input.metadata);
  validateOptionalInteger(input.expiresInMs, 'expiresInMs', 10_000, MAX_JOB_EXPIRY_MS);
  validateOptionalInteger(input.maxAttempts, 'maxAttempts', 1, 10);
  return { ...input, document };
}

function requiredInputString(value: unknown, name: string, maximum: number): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > maximum) {
    throw new TypeError(`${name} must contain between 1 and ${maximum} characters.`);
  }
  return value;
}

function validateOptionalInteger(value: number | undefined, name: string, minimum: number, maximum: number): void {
  if (value !== undefined && (!Number.isSafeInteger(value) || value < minimum || value > maximum)) {
    throw new TypeError(`${name} must be an integer between ${minimum} and ${maximum}.`);
  }
}

function validateMetadata(metadata: Readonly<Record<string, string>> | undefined): void {
  if (metadata === undefined) return;
  const entries = Object.entries(metadata);
  if (entries.length > MAX_METADATA_ENTRIES) {
    throw new TypeError(`metadata cannot contain more than ${MAX_METADATA_ENTRIES} entries.`);
  }
  for (const [key, value] of entries) {
    if (typeof value !== 'string' || key.length > MAX_METADATA_KEY_LENGTH || value.length > MAX_METADATA_VALUE_LENGTH) {
      throw new TypeError('metadata keys and values exceed the OpenPrinter limits.');
    }
  }
}

function parseHealth(value: unknown, method: string, url: string): OpenPrinterHealth {
  if (!isRecord(value) || typeof value.ok !== 'boolean' || typeof value.service !== 'string' || value.service === '') {
    throw invalidResponse(method, url, 'The health response does not match the OpenPrinter contract.');
  }
  return { ok: value.ok, service: value.service };
}

function parseDiscovery(value: unknown, method: string, url: string): OpenPrinterDiscoveryDocument {
  try {
    return parseDiscoveryDocument(value);
  } catch (error) {
    throw invalidResponse(method, url, 'The discovery response does not match the OpenPrinter protocol.', error);
  }
}

function parsePairing(value: unknown, method: string, url: string): OpenPrinterPairingResponse {
  try {
    return parsePairingResponse(value);
  } catch (error) {
    throw invalidResponse(method, url, 'The pairing response does not match the OpenPrinter protocol.', error);
  }
}

function parseCreatePrintJobResponse(value: unknown, method: string, url: string): CreatePrintJobResponse {
  if (!isRecord(value) || typeof value.duplicate !== 'boolean') {
    throw invalidResponse(method, url, 'The create-job response does not match the OpenPrinter contract.');
  }
  return {
    job: parseJob(value.job, method, url),
    duplicate: value.duplicate,
  };
}

function parseListPrintJobsResponse(value: unknown, method: string, url: string): ListPrintJobsResponse {
  if (!isRecord(value) || !Array.isArray(value.jobs) || typeof value.keyPrefix !== 'string') {
    throw invalidResponse(method, url, 'The list-jobs response does not match the OpenPrinter contract.');
  }
  return {
    jobs: value.jobs.map((job) => parseJob(job, method, url)),
    keyPrefix: value.keyPrefix,
  };
}

function parseJob(value: unknown, method: string, url: string): OpenPrinterJob {
  if (!isRecord(value)) {
    throw invalidResponse(method, url, 'The response contains an invalid print job.');
  }

  const state = value.state;
  if (!isPrintJobState(state)) {
    throw invalidResponse(method, url, 'The response contains an unknown print-job state.');
  }

  return {
    id: responseIdentifier(value.id, 'id', method, url),
    projectId: responseIdentifier(value.projectId, 'projectId', method, url),
    printerId: responseIdentifier(value.printerId, 'printerId', method, url),
    idempotencyKey: responseIdentifier(value.idempotencyKey, 'idempotencyKey', method, url),
    state,
    metadata: parseResponseMetadata(value.metadata, method, url),
    expiresAt: responseTimestamp(value.expiresAt, 'expiresAt', method, url),
    attemptCount: responseInteger(value.attemptCount, 'attemptCount', 0, Number.MAX_SAFE_INTEGER, method, url),
    maxAttempts: responseInteger(value.maxAttempts, 'maxAttempts', 1, 10, method, url),
    lastErrorCode: nullableResponseString(value.lastErrorCode, 'lastErrorCode', method, url),
    lastErrorMessage: nullableResponseString(value.lastErrorMessage, 'lastErrorMessage', method, url),
    receivedAt: nullableResponseTimestamp(value.receivedAt, 'receivedAt', method, url),
    submittedAt: nullableResponseTimestamp(value.submittedAt, 'submittedAt', method, url),
    createdAt: responseTimestamp(value.createdAt, 'createdAt', method, url),
    updatedAt: responseTimestamp(value.updatedAt, 'updatedAt', method, url),
  };
}

function parseResponseMetadata(value: unknown, method: string, url: string): Readonly<Record<string, string>> | null {
  if (value === null) return null;
  if (!isRecord(value)) {
    throw invalidResponse(method, url, 'The response contains invalid print-job metadata.');
  }
  const metadata: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item !== 'string') {
      throw invalidResponse(method, url, 'The response contains invalid print-job metadata.');
    }
    metadata[key] = item;
  }
  return metadata;
}

function responseIdentifier(value: unknown, name: string, method: string, url: string): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 256) {
    throw invalidResponse(method, url, `The response contains an invalid ${name}.`);
  }
  return value;
}

function responseString(value: unknown, name: string, method: string, url: string): string {
  if (typeof value !== 'string' || value === '') {
    throw invalidResponse(method, url, `The response contains an invalid ${name}.`);
  }
  return value;
}

function nullableResponseString(value: unknown, name: string, method: string, url: string): string | null {
  if (value === null) return null;
  return responseString(value, name, method, url);
}

function responseTimestamp(value: unknown, name: string, method: string, url: string): string {
  const timestamp = responseString(value, name, method, url);
  if (Number.isNaN(Date.parse(timestamp))) {
    throw invalidResponse(method, url, `The response contains an invalid ${name} timestamp.`);
  }
  return timestamp;
}

function nullableResponseTimestamp(value: unknown, name: string, method: string, url: string): string | null {
  if (value === null) return null;
  return responseTimestamp(value, name, method, url);
}

function responseInteger(
  value: unknown,
  name: string,
  minimum: number,
  maximum: number,
  method: string,
  url: string,
): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw invalidResponse(method, url, `The response contains an invalid ${name}.`);
  }
  return value;
}

function isPrintJobState(value: unknown): value is OpenPrinterJob['state'] {
  return typeof value === 'string' && (OPENPRINTER_PRINT_JOB_STATES as readonly string[]).includes(value);
}

function invalidResponse(method: string, url: string, message: string, cause?: unknown): OpenPrinterResponseError {
  return new OpenPrinterResponseError(method, url, message, cause);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === 'string' && value !== '' ? value : undefined;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
