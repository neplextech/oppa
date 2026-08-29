import type {
  OpenPrinterDiscoveryDocument,
  OpenPrinterPairingRequest,
  OpenPrinterPairingResponse,
  PrintDocument,
} from '@openprinter/protocol';

/** Public scopes supported by a Neplex OpenPrinter Cloud project API key. */
export const OPENPRINTER_API_KEY_SCOPES = ['jobs:read', 'jobs:write'] as const;

/** A scope accepted by the public OpenPrinter Cloud API. */
export type OpenPrinterApiKeyScope = (typeof OPENPRINTER_API_KEY_SCOPES)[number];

/** Print-job states accepted by the public list endpoint. */
export const OPENPRINTER_PRINT_JOB_STATES = [
  'PENDING',
  'DISPATCHING',
  'DELIVERED',
  'PRINTING',
  'COMPLETED',
  'FAILED',
  'EXPIRED',
  'CANCELLED',
] as const;

/** A print-job state returned by Neplex OpenPrinter Cloud. */
export type OpenPrinterPrintJobState = (typeof OPENPRINTER_PRINT_JOB_STATES)[number];

/** A provider for an API key, useful when secrets rotate at runtime. */
export type OpenPrinterApiKey = string | (() => string | Promise<string>);

/** Retry behavior for transient transport and HTTP failures. */
export interface OpenPrinterRetryOptions {
  /** Number of retries after the initial request. Defaults to `2`. */
  readonly maxRetries?: number;
  /** Initial delay between retries in milliseconds. Defaults to `250`. */
  readonly baseDelayMs?: number;
  /** Upper bound for exponential backoff in milliseconds. Defaults to `5_000`. */
  readonly maxDelayMs?: number;
}

/** Configuration for a typed OpenPrinter Cloud client. */
export interface OpenPrinterClientOptions {
  /** Service origin or mounted base path; defaults to OpenPrinter environment variables. */
  readonly baseUrl?: string;
  /** Project API key or a provider that returns the current key. */
  readonly apiKey?: OpenPrinterApiKey;
  /** Default project ID used by `client.jobs`; defaults to `OPENPRINTER_PROJECT_ID`. */
  readonly projectId?: string;
  /** Fetch implementation for Node, browsers, tests, or custom runtimes. */
  readonly fetch?: typeof globalThis.fetch;
  /** Request deadline in milliseconds. Defaults to `30_000`. */
  readonly timeoutMs?: number;
  /** Set to `false` to disable retries, or configure transient retry behavior. */
  readonly retry?: false | OpenPrinterRetryOptions;
  /** Additional headers sent with every request. Authorization is managed by the client. */
  readonly headers?: HeadersInit;
}

/** Options shared by non-project-specific requests. */
export interface OpenPrinterRequestOptions {
  /** Abort the request and any pending retry delay. */
  readonly signal?: AbortSignal;
}

/** Input for creating one durable print job. */
export interface CreatePrintJobInput {
  /** The project-local printer record to receive the job. */
  readonly printerId: string;
  /** Stable key used to make retries safe and deduplicate submissions. */
  readonly idempotencyKey: string;
  /** Structured, printer-independent receipt document. */
  readonly document: PrintDocument;
  /** Optional bounded application metadata echoed in job responses. */
  readonly metadata?: Readonly<Record<string, string>>;
  /** Job expiry in milliseconds; the service accepts 10 seconds to 30 days. */
  readonly expiresInMs?: number;
  /** Maximum delivery attempts; the service accepts 1 through 10. */
  readonly maxAttempts?: number;
}

/** Options for listing a project’s print jobs. */
export interface ListPrintJobsOptions extends OpenPrinterRequestOptions {
  /** Filter by one lifecycle state. */
  readonly state?: OpenPrinterPrintJobState;
}

/** Public job projection returned by OpenPrinter Cloud. */
export interface OpenPrinterJob {
  readonly id: string;
  readonly projectId: string;
  readonly printerId: string;
  readonly idempotencyKey: string;
  readonly state: OpenPrinterPrintJobState;
  readonly metadata: Readonly<Record<string, string>> | null;
  readonly expiresAt: string;
  readonly attemptCount: number;
  readonly maxAttempts: number;
  readonly lastErrorCode: string | null;
  readonly lastErrorMessage: string | null;
  readonly receivedAt: string | null;
  readonly submittedAt: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/** Response returned when a print job is created or an idempotency key repeats. */
export interface CreatePrintJobResponse {
  readonly job: OpenPrinterJob;
  readonly duplicate: boolean;
}

/** Response returned by the public print-job list endpoint. */
export interface ListPrintJobsResponse {
  readonly jobs: readonly OpenPrinterJob[];
  /** Prefix of the API key used for the request, safe to display in diagnostics. */
  readonly keyPrefix: string;
}

/** Health response from the OpenPrinter service. */
export interface OpenPrinterHealth {
  readonly ok: boolean;
  readonly service: string;
}

/** Project-scoped job methods. */
export interface OpenPrinterJobsResource {
  /** Submit a durable job. The SDK retries this request because the idempotency key makes it safe. */
  create(input: CreatePrintJobInput, options?: OpenPrinterRequestOptions): Promise<CreatePrintJobResponse>;
  /** List public job projections, optionally filtered by lifecycle state. */
  list(options?: ListPrintJobsOptions): Promise<ListPrintJobsResponse>;
}

/** A client view pinned to one OpenPrinter Cloud project. */
export interface OpenPrinterProjectClient {
  readonly projectId: string;
  readonly jobs: OpenPrinterJobsResource;
}

/** Strongly typed client for the public Neplex OpenPrinter Cloud API. */
export interface OpenPrinterClient {
  /** Normalized service base URL used by this client. */
  readonly baseUrl: string;
  /** Configured default project, or `undefined` when each call must use `client.project()`. */
  readonly projectId: string | undefined;
  /** Jobs resource using the configured default project. */
  readonly jobs: OpenPrinterJobsResource;
  /** Pin a resource view to a project without changing the client configuration. */
  project(projectId: string): OpenPrinterProjectClient;
  /** Read service liveness without an API key. */
  health(options?: OpenPrinterRequestOptions): Promise<OpenPrinterHealth>;
  /** Read and validate the standard OpenPrinter discovery document. */
  discovery(options?: OpenPrinterRequestOptions): Promise<OpenPrinterDiscoveryDocument>;
  /** Submit an agent pairing request; key generation and signing remain agent-owned. */
  pair(input: OpenPrinterPairingRequest, options?: OpenPrinterRequestOptions): Promise<OpenPrinterPairingResponse>;
}
