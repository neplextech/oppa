import { Type, type Static } from '@sinclair/typebox';

import { IdentifierSchema, MetadataSchema, TimestampSchema } from './common.js';
import { PrintDocumentSchema } from './document.js';
import { PdfDocumentSchema } from './pdf.js';

/**
 * A concrete, idempotent delivery request for one OPPA printer.
 *
 * Reprints require a new `jobId`; hosts may preserve their relationship in
 * opaque metadata.
 */
export const PrintJobSchema = Type.Object(
  {
    jobId: IdentifierSchema,
    idempotencyKey: Type.String({ minLength: 1, maxLength: 256 }),
    printerId: IdentifierSchema,
    createdAt: TimestampSchema,
    /** A structured receipt, or a PDF referenced by URL with integrity metadata. */
    document: Type.Union([PrintDocumentSchema, PdfDocumentSchema]),
    metadata: Type.Optional(MetadataSchema),
  },
  {
    additionalProperties: false,
    title: 'PrintJob',
  },
);

/** TypeScript representation of a validated print job. */
export type PrintJob = Static<typeof PrintJobSchema>;
