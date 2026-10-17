import { FormatRegistry, Type, type Static } from '@sinclair/typebox';

import {
  MAX_COPIES,
  MAX_CUSTOM_PAPER_MM,
  MAX_PDF_DOCUMENT_BYTES,
  MAX_PDF_PAGE_RANGE_BYTES,
  MAX_PDF_URL_BYTES,
  MAX_SCALE_PERCENT,
  MIN_COPIES,
  MIN_CUSTOM_PAPER_MM,
  MIN_SCALE_PERCENT,
} from '../constants.js';

/** Format name used for page-range expressions, whose ordering rules exceed a regular expression. */
export const PAGE_RANGES_FORMAT = 'openprinter-page-ranges';

/** Format name used for PDF retrieval URLs. */
export const DOCUMENT_URL_FORMAT = 'openprinter-document-url';

const PAGE_NUMBER_PATTERN = /^[0-9]{1,5}$/;

/**
 * Validates a page-range expression such as `1-3,5`.
 *
 * Ranges must ascend, page numbers start at 1, and each number has at most five digits.
 */
export function isValidPageRanges(value: string): boolean {
  if (value.length === 0 || value.length > MAX_PDF_PAGE_RANGE_BYTES) {
    return false;
  }
  return value.split(',').every((part) => {
    const pieces = part.split('-');
    if (pieces.length > 2) {
      return false;
    }
    const [start, end = start] = pieces;
    if (start === undefined || end === undefined) {
      return false;
    }
    if (!PAGE_NUMBER_PATTERN.test(start) || !PAGE_NUMBER_PATTERN.test(end)) {
      return false;
    }
    const first = Number(start);
    const last = Number(end);
    return first >= 1 && first <= last;
  });
}

/**
 * Validates a PDF retrieval URL.
 *
 * HTTPS hosts are plain host names or IPv4 addresses. Plain HTTP is accepted only for loopback hosts.
 * Credentials, IPv6 literals, fragments, spaces, and control characters are rejected.
 */
export function isValidDocumentUrl(value: string): boolean {
  if (value.length > MAX_PDF_URL_BYTES) {
    return false;
  }
  const pattern = new RegExp(
    '^(?:https://[A-Za-z0-9.-]+(?::[0-9]{1,5})?|http://(?:localhost|127\\.0\\.0\\.1)(?::[0-9]{1,5})?)' +
      '(?:[/?][^ #\\u0000-\\u001F\\u007F]*)?$',
  );
  return pattern.test(value);
}

FormatRegistry.Set(PAGE_RANGES_FORMAT, isValidPageRanges);
FormatRegistry.Set(DOCUMENT_URL_FORMAT, isValidDocumentUrl);

/** Discriminator selecting the PDF document representation. */
export const PdfFormatSchema = Type.Literal('pdf');

/** Standard paper names with reliable meaning in every backend. */
export const StandardPaperSizeSchema = Type.Union([
  Type.Literal('a4'),
  Type.Literal('a5'),
  Type.Literal('letter'),
  Type.Literal('legal'),
]);

/** A standard paper name. */
export type StandardPaperSize = Static<typeof StandardPaperSizeSchema>;

/** Custom media dimensions in millimetres, such as label stock. */
export const CustomPaperSizeSchema = Type.Object(
  {
    widthMm: Type.Integer({ minimum: MIN_CUSTOM_PAPER_MM, maximum: MAX_CUSTOM_PAPER_MM }),
    heightMm: Type.Integer({ minimum: MIN_CUSTOM_PAPER_MM, maximum: MAX_CUSTOM_PAPER_MM }),
  },
  { additionalProperties: false },
);

/** Requested paper size: a standard name or explicit millimetre dimensions. */
export const PaperSizeSchema = Type.Union([StandardPaperSizeSchema, CustomPaperSizeSchema]);

/** Requested paper size. */
export type PaperSize = Static<typeof PaperSizeSchema>;

/** Page orientation applied before physical placement. */
export const OrientationSchema = Type.Union([Type.Literal('portrait'), Type.Literal('landscape')]);

/** Page orientation. */
export type Orientation = Static<typeof OrientationSchema>;

/** Scaling: fit the page to the paper, print at actual size, or print at a percentage. */
export const ScalingSchema = Type.Union([
  Type.Literal('fit'),
  Type.Literal('actual'),
  Type.Object(
    {
      percent: Type.Integer({ minimum: MIN_SCALE_PERCENT, maximum: MAX_SCALE_PERCENT }),
    },
    { additionalProperties: false },
  ),
]);

/** How a page is scaled to the selected paper. */
export type Scaling = Static<typeof ScalingSchema>;

/** Requested duplex mode. Backends that cannot honour it fail the job. */
export const DuplexSchema = Type.Union([
  Type.Literal('simplex'),
  Type.Literal('long-edge'),
  Type.Literal('short-edge'),
]);

/** Requested duplex mode. */
export type Duplex = Static<typeof DuplexSchema>;

/** Print settings for a document. Every omitted field uses its documented default. */
export const PrintSettingsSchema = Type.Object(
  {
    /** Number of copies, from 1 to 99. Defaults to 1. */
    copies: Type.Optional(Type.Integer({ minimum: MIN_COPIES, maximum: MAX_COPIES })),
    /** Target paper. Defaults to each page's own media size. */
    paper: Type.Optional(PaperSizeSchema),
    /** Page orientation. Defaults to the document's own orientation. */
    orientation: Type.Optional(OrientationSchema),
    /** Scaling mode. Defaults to `actual`. */
    scaling: Type.Optional(ScalingSchema),
    /** Page selection such as `1-3,5`. Defaults to every page in order. */
    pageRanges: Type.Optional(
      Type.String({
        minLength: 1,
        maxLength: MAX_PDF_PAGE_RANGE_BYTES,
        format: PAGE_RANGES_FORMAT,
      }),
    ),
    /** Duplex mode. Defaults to single-sided. */
    duplex: Type.Optional(DuplexSchema),
  },
  { additionalProperties: false },
);

/** Print settings for a document. */
export type PrintSettings = Static<typeof PrintSettingsSchema>;

/**
 * A PDF retrieved by the agent over an authenticated request.
 *
 * Bytes never travel inside protocol messages. The agent verifies `byteLength` and `sha256` before
 * rendering the document.
 */
export const PdfSourceSchema = Type.Object(
  {
    type: Type.Literal('url'),
    url: Type.String({ minLength: 1, maxLength: MAX_PDF_URL_BYTES, format: DOCUMENT_URL_FORMAT }),
    sha256: Type.String({ pattern: '^[0-9a-f]{64}$' }),
    byteLength: Type.Integer({ minimum: 1, maximum: MAX_PDF_DOCUMENT_BYTES }),
  },
  { additionalProperties: false },
);

/** A PDF retrieval reference. */
export type PdfSource = Static<typeof PdfSourceSchema>;

/** A PDF document reference plus its print settings. */
export const PdfDocumentSchema = Type.Object(
  {
    format: PdfFormatSchema,
    source: PdfSourceSchema,
    settings: Type.Optional(PrintSettingsSchema),
  },
  {
    additionalProperties: false,
    title: 'PdfDocument',
  },
);

/** TypeScript representation of a PDF document reference. */
export type PdfDocument = Static<typeof PdfDocumentSchema>;
