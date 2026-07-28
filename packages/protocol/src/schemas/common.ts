import { Type, type Static, type TLiteral } from '@sinclair/typebox';

import { MAX_METADATA_ENTRIES, PROTOCOL_VERSION } from '../constants.js';

function integerLiteral<const TValue extends number>(value: TValue): TLiteral<TValue> {
  const schema = Type.Literal(value);
  (schema as unknown as { type: string }).type = 'integer';
  return schema;
}

/** A stable opaque identifier used on the wire. */
export const IdentifierSchema = Type.String({
  minLength: 1,
  maxLength: 128,
  pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$',
});

/** TypeScript representation of a validated protocol identifier. */
export type Identifier = Static<typeof IdentifierSchema>;

/**
 * UTC RFC 3339 timestamp accepted by both protocol implementations.
 *
 * The protocol deliberately requires `Z` rather than accepting local offsets.
 */
export const TimestampSchema = Type.String({
  minLength: 20,
  maxLength: 30,
  pattern: '^\\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\\d|3[01])T(?:[01]\\d|2[0-3]):[0-5]\\d:[0-5]\\d(?:\\.\\d{1,9})?Z$',
});

/** TypeScript representation of a validated UTC timestamp. */
export type Timestamp = Static<typeof TimestampSchema>;

/** A bounded human-readable wire string. */
export const ShortStringSchema = Type.String({
  minLength: 1,
  maxLength: 256,
});

/** Opaque metadata with bounded keys, values, and entry count. */
export const MetadataSchema = Type.Record(
  Type.String({
    minLength: 1,
    maxLength: 64,
    pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,63}$',
  }),
  Type.String({ maxLength: 1024 }),
  {
    additionalProperties: false,
    maxProperties: MAX_METADATA_ENTRIES,
  },
);

/** TypeScript representation of validated opaque metadata. */
export type Metadata = Static<typeof MetadataSchema>;

/** All lifecycle terms reserved by OpenPrinter v1. */
export const JobStatusSchema = Type.Union([
  Type.Literal('queued'),
  Type.Literal('delivered'),
  Type.Literal('received'),
  Type.Literal('submitted'),
  Type.Literal('failed'),
  Type.Literal('cancelled'),
]);

/** A lifecycle status that never implies verified physical output. */
export type JobStatus = Static<typeof JobStatusSchema>;

/** The singleton protocol version schema used by every v1 envelope. */
export const ProtocolVersionSchema = integerLiteral(PROTOCOL_VERSION);

/** TypeScript representation of the current protocol version. */
export type ProtocolVersion = Static<typeof ProtocolVersionSchema>;
