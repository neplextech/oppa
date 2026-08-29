/** Configuration failure raised before a request can be sent. */
export class OpenPrinterConfigurationError extends Error {
  public constructor(message: string) {
    super(message);
    this.name = 'OpenPrinterConfigurationError';
  }
}

/** Options captured on a failed HTTP request. */
export interface OpenPrinterApiErrorOptions {
  readonly status: number;
  readonly method: string;
  readonly url: string;
  readonly code?: string;
  readonly message?: string;
  readonly requestId?: string;
  readonly details?: unknown;
}

/** An HTTP error returned by the OpenPrinter Cloud API. */
export class OpenPrinterApiError extends Error {
  public readonly status: number;
  public readonly method: string;
  public readonly url: string;
  public readonly code: string | undefined;
  public readonly requestId: string | undefined;
  public readonly details: unknown;

  public constructor(options: OpenPrinterApiErrorOptions) {
    super(options.message ?? `OpenPrinter request failed with HTTP ${options.status}.`);
    this.name = 'OpenPrinterApiError';
    this.status = options.status;
    this.method = options.method;
    this.url = options.url;
    this.code = options.code;
    this.requestId = options.requestId;
    this.details = options.details;
  }
}

/** A successful HTTP response that did not match the documented response contract. */
export class OpenPrinterResponseError extends Error {
  public readonly method: string;
  public readonly url: string;
  public readonly cause: unknown;

  public constructor(method: string, url: string, message: string, cause: unknown = undefined) {
    super(message);
    this.name = 'OpenPrinterResponseError';
    this.method = method;
    this.url = url;
    this.cause = cause;
  }
}

/** Request deadline failure. A retry may still occur when configured. */
export class OpenPrinterTimeoutError extends Error {
  public readonly timeoutMs: number;
  public readonly method: string;
  public readonly url: string;

  public constructor(method: string, url: string, timeoutMs: number) {
    super(`OpenPrinter request exceeded its ${timeoutMs}ms timeout.`);
    this.name = 'OpenPrinterTimeoutError';
    this.timeoutMs = timeoutMs;
    this.method = method;
    this.url = url;
  }
}

/** Type guard for callers that need to branch on an HTTP API failure. */
export function isOpenPrinterApiError(error: unknown): error is OpenPrinterApiError {
  return error instanceof OpenPrinterApiError;
}

/** Type guard for a successful response that violated the documented contract. */
export function isOpenPrinterResponseError(error: unknown): error is OpenPrinterResponseError {
  return error instanceof OpenPrinterResponseError;
}

/** Type guard for a request deadline failure. */
export function isOpenPrinterTimeoutError(error: unknown): error is OpenPrinterTimeoutError {
  return error instanceof OpenPrinterTimeoutError;
}

/** Type guard for client configuration failures. */
export function isOpenPrinterConfigurationError(error: unknown): error is OpenPrinterConfigurationError {
  return error instanceof OpenPrinterConfigurationError;
}
