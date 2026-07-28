import { Type, type Static } from '@sinclair/typebox';

import { IdentifierSchema, ShortStringSchema } from './common.js';
import { ReceiptWidthSchema } from './document.js';

/** How OPPA addresses a printer locally. */
export const PrinterConnectionSchema = Type.Union([
  Type.Object(
    {
      type: Type.Literal('system'),
      systemName: Type.String({ minLength: 1, maxLength: 256 }),
    },
    { additionalProperties: false },
  ),
  Type.Object(
    {
      type: Type.Literal('tcp'),
      host: Type.String({ minLength: 1, maxLength: 253 }),
      port: Type.Integer({ minimum: 1, maximum: 65_535 }),
    },
    { additionalProperties: false },
  ),
  Type.Object({ type: Type.Literal('virtual') }, { additionalProperties: false }),
]);

/** TypeScript representation of a validated local printer connection. */
export type PrinterConnection = Static<typeof PrinterConnectionSchema>;

/** Rendering and submission features reported by a printer backend. */
export const PrinterCapabilitiesSchema = Type.Object(
  {
    mediaWidths: Type.Array(ReceiptWidthSchema, {
      minItems: 1,
      maxItems: 2,
      uniqueItems: true,
    }),
    raster: Type.Boolean(),
    cut: Type.Boolean(),
    qr: Type.Boolean(),
    barcode: Type.Boolean(),
  },
  { additionalProperties: false },
);

/** TypeScript representation of validated printer capabilities. */
export type PrinterCapabilities = Static<typeof PrinterCapabilitiesSchema>;

/** Current reachability state reported by OPPA. */
export const PrinterAvailabilitySchema = Type.Union([
  Type.Literal('online'),
  Type.Literal('offline'),
  Type.Literal('unknown'),
]);

/** Current reachability state reported by OPPA. */
export type PrinterAvailability = Static<typeof PrinterAvailabilitySchema>;

const descriptorFields = {
  id: IdentifierSchema,
  fingerprint: Type.String({ minLength: 1, maxLength: 256 }),
  name: ShortStringSchema,
  capabilities: Type.Optional(PrinterCapabilitiesSchema),
  enabled: Type.Boolean(),
  availability: PrinterAvailabilitySchema,
};

/** A physical or virtual printer exposed by one OPPA agent. */
export const PrinterDescriptorSchema = Type.Union(
  [
    Type.Object(
      {
        ...descriptorFields,
        kind: Type.Literal('local'),
        connection: Type.Object(
          {
            type: Type.Literal('system'),
            systemName: Type.String({ minLength: 1, maxLength: 256 }),
          },
          { additionalProperties: false },
        ),
      },
      { additionalProperties: false },
    ),
    Type.Object(
      {
        ...descriptorFields,
        kind: Type.Literal('network'),
        connection: Type.Object(
          {
            type: Type.Literal('tcp'),
            host: Type.String({ minLength: 1, maxLength: 253 }),
            port: Type.Integer({ minimum: 1, maximum: 65_535 }),
          },
          { additionalProperties: false },
        ),
      },
      { additionalProperties: false },
    ),
    Type.Object(
      {
        ...descriptorFields,
        kind: Type.Literal('virtual'),
        connection: Type.Object({ type: Type.Literal('virtual') }, { additionalProperties: false }),
      },
      { additionalProperties: false },
    ),
  ],
  { title: 'PrinterDescriptor' },
);

/** TypeScript representation of a validated printer descriptor. */
export type PrinterDescriptor = Static<typeof PrinterDescriptorSchema>;
