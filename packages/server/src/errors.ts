import type { DeliveryFailure, DeliveryResult, DeliverySuccess } from './types.js';

/** Stable configuration error raised before any protocol session starts. */
export class OpenPrinterServerConfigurationError extends Error {
  /** Machine-readable configuration category. */
  public readonly code = 'INVALID_SERVER_CONFIGURATION' as const;

  public constructor(message: string) {
    super(message);
    this.name = 'OpenPrinterServerConfigurationError';
  }
}

/**
 * Exception form of a structured delivery failure.
 *
 * The SDK's delivery methods return results by default. Use
 * `deliveryResultOrThrow` when an exception-oriented host API is preferable.
 */
export class OpenPrinterDeliveryError extends Error {
  /** Machine-readable delivery failure reason. */
  public readonly code: DeliveryFailure['reason'];
  /** Agent whose selected session did not accept the handoff. */
  public readonly agentId: string;
  /** A later application-owned retry may succeed. */
  public readonly retryable = true as const;

  public constructor(failure: DeliveryFailure) {
    super(`OpenPrinter message was not delivered to ${failure.agentId}: ${failure.reason}`);
    this.name = 'OpenPrinterDeliveryError';
    this.code = failure.reason;
    this.agentId = failure.agentId;
  }
}

/** Return a successful delivery or throw its structured exception form. */
export function deliveryResultOrThrow(result: DeliveryResult): DeliverySuccess {
  if (!result.ok) {
    throw new OpenPrinterDeliveryError(result);
  }

  return result;
}
