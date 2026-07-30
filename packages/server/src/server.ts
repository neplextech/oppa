import { randomUUID } from 'node:crypto';

import { validateAcceptInput, validateOptions } from './internal.js';
import { OpenPrinterSessionImplementation } from './session.js';
import type {
  AcceptOpenPrinterSessionInput,
  OpenPrinterServer,
  OpenPrinterServerOptions,
  OpenPrinterSession,
} from './types.js';

/**
 * Create a framework-neutral OpenPrinter protocol session factory.
 *
 * The host owns authentication, HTTP/WebSocket or broker lifecycle, live
 * connection routing, and any cluster coordination. The returned factory only
 * creates independent protocol sessions over host-supplied transports.
 */
export function createOpenPrinterServer<Metadata>(
  options: OpenPrinterServerOptions<Metadata>,
): OpenPrinterServer<Metadata> {
  const resolved = validateOptions(options);

  return {
    accept(input: AcceptOpenPrinterSessionInput<Metadata>): OpenPrinterSession<Metadata> {
      validateAcceptInput(input);
      return new OpenPrinterSessionImplementation(options, resolved, input, input.sessionId ?? randomUUID());
    },
  };
}
