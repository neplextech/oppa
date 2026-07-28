use std::{error::Error, fmt};

/// A path-scoped semantic validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Dot-separated path to the invalid field.
    pub path: String,
    /// Payload-free explanation of the violated invariant.
    pub message: String,
}

impl ValidationError {
    /// Creates a validation error without retaining the rejected value.
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Prefixes the error path while propagating a nested validation failure.
    pub(crate) fn at(self, prefix: impl AsRef<str>) -> Self {
        let prefix = prefix.as_ref();
        let path = if self.path.is_empty() {
            prefix.to_owned()
        } else {
            format!("{prefix}.{}", self.path)
        };
        Self { path, ..self }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl Error for ValidationError {}

/// Expected failure while decoding, validating, or encoding a wire message.
#[derive(Debug)]
pub enum ProtocolError {
    /// Input is not syntactically valid UTF-8 JSON.
    InvalidJson,
    /// A JSON value does not satisfy the canonical message contract.
    InvalidMessage(ValidationError),
    /// The encoded message exceeds the hard transport limit.
    MessageTooLarge {
        /// Observed UTF-8 byte count.
        actual: usize,
        /// Maximum accepted UTF-8 byte count.
        limit: usize,
    },
    /// The envelope requests a version this crate cannot interpret.
    UnsupportedProtocolVersion {
        /// Primitive version value rendered without the rest of the payload.
        received: String,
        /// Versions understood by this crate.
        supported: &'static [u16],
    },
    /// Serialization failed after semantic validation.
    Encoding(String),
}

impl ProtocolError {
    /// Stable machine-readable category aligned with the TypeScript codecs.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::InvalidMessage(_) => "invalid_message",
            Self::MessageTooLarge { .. } => "message_too_large",
            Self::UnsupportedProtocolVersion { .. } => "unsupported_protocol_version",
            Self::Encoding(_) => "invalid_message",
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => formatter.write_str("protocol message is not valid UTF-8 JSON"),
            Self::InvalidMessage(error) => write!(formatter, "invalid protocol message: {error}"),
            Self::MessageTooLarge { actual, limit } => {
                write!(
                    formatter,
                    "protocol message is {actual} bytes; limit is {limit} bytes"
                )
            }
            Self::UnsupportedProtocolVersion {
                received,
                supported,
            } => write!(
                formatter,
                "unsupported protocol version {received}; supported versions: {supported:?}"
            ),
            Self::Encoding(message) => {
                write!(formatter, "could not encode protocol message: {message}")
            }
        }
    }
}

impl Error for ProtocolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidMessage(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ValidationError> for ProtocolError {
    fn from(error: ValidationError) -> Self {
        Self::InvalidMessage(error)
    }
}
