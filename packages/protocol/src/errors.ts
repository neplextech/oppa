/** Machine-readable validation issue independent of TypeBox internals. */
export interface ProtocolIssue {
  /** JSON Pointer-like path to the rejected value. */
  readonly path: string;
  /** Human-readable validation failure. */
  readonly message: string;
}

/** Stable error categories produced by protocol codecs. */
export type ProtocolErrorCode =
  | 'invalid_json'
  | 'invalid_message'
  | 'message_too_large'
  | 'unsupported_protocol_version';

/** Base error for all expected protocol decoding and validation failures. */
export class ProtocolError extends Error {
  /** Stable category suitable for transport error mapping. */
  readonly code: ProtocolErrorCode;

  /**
   * Creates a protocol error without retaining the rejected payload.
   *
   * Avoiding payload retention prevents accidental diagnostic disclosure.
   */
  constructor(code: ProtocolErrorCode, message: string) {
    super(message);
    this.name = 'ProtocolError';
    this.code = code;
  }
}

/** A structurally invalid message with bounded, payload-free issue details. */
export class ProtocolValidationError extends ProtocolError {
  /** Validation failures reported by the canonical schema. */
  readonly issues: readonly ProtocolIssue[];

  /** Creates an invalid-message error from normalized schema issues. */
  constructor(message: string, issues: readonly ProtocolIssue[] = []) {
    super('invalid_message', message);
    this.name = 'ProtocolValidationError';
    this.issues = issues;
  }
}

/** A syntactically valid envelope that requests an unsupported version. */
export class UnsupportedProtocolVersionError extends ProtocolError {
  /** The rejected value, retained only when it is a primitive. */
  readonly receivedVersion: string | number | boolean | null;

  /** Protocol versions supported by this package. */
  readonly supportedVersions: readonly number[];

  /** Creates an explicit version-negotiation failure. */
  constructor(receivedVersion: string | number | boolean | null) {
    super(
      'unsupported_protocol_version',
      `Unsupported protocol version ${String(receivedVersion)}; supported versions: 1`,
    );
    this.name = 'UnsupportedProtocolVersionError';
    this.receivedVersion = receivedVersion;
    this.supportedVersions = [1];
  }
}
