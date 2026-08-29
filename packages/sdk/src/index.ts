/**
 * Public TypeScript client for the Neplex OpenPrinter Cloud API.
 *
 * The SDK intentionally follows the public API-key surface. Agent pairing and
 * gateway authentication remain separate protocol concerns owned by OPPA.
 */
export { createOpenPrinterClient } from './client.js';
export {
  OpenPrinterApiError,
  OpenPrinterConfigurationError,
  OpenPrinterResponseError,
  OpenPrinterTimeoutError,
  isOpenPrinterApiError,
  isOpenPrinterConfigurationError,
  isOpenPrinterResponseError,
  isOpenPrinterTimeoutError,
} from './errors.js';
export {
  OPENPRINTER_API_KEY_SCOPES,
  OPENPRINTER_PRINT_JOB_STATES,
  type CreatePrintJobInput,
  type CreatePrintJobResponse,
  type ListPrintJobsOptions,
  type ListPrintJobsResponse,
  type OpenPrinterApiKey,
  type OpenPrinterApiKeyScope,
  type OpenPrinterClient,
  type OpenPrinterClientOptions,
  type OpenPrinterHealth,
  type OpenPrinterJob,
  type OpenPrinterJobsResource,
  type OpenPrinterPrintJobState,
  type OpenPrinterProjectClient,
  type OpenPrinterRequestOptions,
  type OpenPrinterRetryOptions,
} from './types.js';
