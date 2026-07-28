import { Type, type Static, type TLiteral } from '@sinclair/typebox';

import { MAX_DOCUMENT_SECTIONS, MAX_IMAGE_BASE64_LENGTH } from '../constants.js';

function integerLiteral<const TValue extends number>(value: TValue): TLiteral<TValue> {
  const schema = Type.Literal(value);
  (schema as unknown as { type: string }).type = 'integer';
  return schema;
}

/** Supported receipt media widths, expressed in millimetres. */
export const ReceiptWidthSchema = Type.Union([integerLiteral(58), integerLiteral(80)]);

/** A supported receipt media width in millimetres. */
export type ReceiptWidth = Static<typeof ReceiptWidthSchema>;

/** Horizontal alignment for a text primitive. */
export const TextAlignmentSchema = Type.Union([Type.Literal('left'), Type.Literal('center'), Type.Literal('right')]);

/** Horizontal alignment for a text primitive. */
export type TextAlignment = Static<typeof TextAlignmentSchema>;

/** A bounded text block. */
export const TextSectionSchema = Type.Object(
  {
    type: Type.Literal('text'),
    value: Type.String({ maxLength: 16_384 }),
    align: Type.Optional(TextAlignmentSchema),
    bold: Type.Optional(Type.Boolean()),
  },
  { additionalProperties: false },
);

/** A two-column text row whose layout is renderer-owned. */
export const RowSectionSchema = Type.Object(
  {
    type: Type.Literal('row'),
    left: Type.String({ maxLength: 4_096 }),
    right: Type.String({ maxLength: 4_096 }),
  },
  { additionalProperties: false },
);

/** A renderer-selected horizontal divider. */
export const DividerSectionSchema = Type.Object({ type: Type.Literal('divider') }, { additionalProperties: false });

/**
 * A raster image transported as bounded base64.
 *
 * Data URIs and local paths are intentionally not accepted.
 */
export const ImageSectionSchema = Type.Object(
  {
    type: Type.Literal('image'),
    mediaType: Type.Union([Type.Literal('image/png'), Type.Literal('image/jpeg')]),
    data: Type.String({
      minLength: 4,
      maxLength: MAX_IMAGE_BASE64_LENGTH,
      pattern: '^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$',
    }),
  },
  { additionalProperties: false },
);

/** A QR code primitive. */
export const QrSectionSchema = Type.Object(
  {
    type: Type.Literal('qr'),
    value: Type.String({ minLength: 1, maxLength: 4_096 }),
  },
  { additionalProperties: false },
);

/** Supported linear barcode symbologies. */
export const BarcodeFormatSchema = Type.Union([
  Type.Literal('code128'),
  Type.Literal('code39'),
  Type.Literal('ean13'),
  Type.Literal('upca'),
]);

/** Supported linear barcode symbology. */
export type BarcodeFormat = Static<typeof BarcodeFormatSchema>;

/** A linear barcode primitive with an explicit symbology. */
export const BarcodeSectionSchema = Type.Object(
  {
    type: Type.Literal('barcode'),
    format: BarcodeFormatSchema,
    value: Type.String({
      minLength: 1,
      maxLength: 256,
      pattern: '^[\\x20-\\x7E]+$',
    }),
  },
  { additionalProperties: false },
);

/** A bounded paper-feed operation. */
export const FeedSectionSchema = Type.Object(
  {
    type: Type.Literal('feed'),
    lines: Type.Integer({ minimum: 1, maximum: 255 }),
  },
  { additionalProperties: false },
);

/** A cut operation; the renderer selects a supported cut mode. */
export const CutSectionSchema = Type.Object({ type: Type.Literal('cut') }, { additionalProperties: false });

/** Any structured document primitive supported by protocol v1. */
export const PrintSectionSchema = Type.Union([
  TextSectionSchema,
  RowSectionSchema,
  DividerSectionSchema,
  ImageSectionSchema,
  QrSectionSchema,
  BarcodeSectionSchema,
  FeedSectionSchema,
  CutSectionSchema,
]);

/** TypeScript representation of a structured document primitive. */
export type PrintSection = Static<typeof PrintSectionSchema>;

/**
 * Printer-independent structured receipt content.
 *
 * Rendering and physical submission are outside the protocol package.
 */
export const PrintDocumentSchema = Type.Object(
  {
    width: ReceiptWidthSchema,
    sections: Type.Array(PrintSectionSchema, {
      minItems: 1,
      maxItems: MAX_DOCUMENT_SECTIONS,
    }),
  },
  {
    additionalProperties: false,
    title: 'PrintDocument',
  },
);

/** TypeScript representation of a validated structured print document. */
export type PrintDocument = Static<typeof PrintDocumentSchema>;
