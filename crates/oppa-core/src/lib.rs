//! Shared, dependency-light domain primitives for OPPA.
//!
//! This crate deliberately contains no network, database, printer, or desktop
//! integration. Higher-level crates use its identifiers and lifecycle enums to
//! keep boundaries explicit without depending on one another.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum encoded length accepted for an OPPA identifier.
pub const MAX_IDENTIFIER_LEN: usize = 128;

/// Describes why an identifier could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentifierError {
    /// The identifier was empty or contained only whitespace.
    #[error("identifier must not be empty")]
    Empty,
    /// The identifier exceeded [`MAX_IDENTIFIER_LEN`].
    #[error("identifier is {actual} bytes; the maximum is {maximum}")]
    TooLong {
        /// Actual UTF-8 byte length.
        actual: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// The identifier contained a control character.
    #[error("identifier must not contain control characters")]
    ControlCharacter,
    /// Whitespace appeared at either edge.
    #[error("identifier must not have leading or trailing whitespace")]
    SurroundingWhitespace,
}

fn validate_identifier(value: &str) -> Result<(), IdentifierError> {
    if value.trim().is_empty() {
        return Err(IdentifierError::Empty);
    }
    if value.len() > MAX_IDENTIFIER_LEN {
        return Err(IdentifierError::TooLong {
            actual: value.len(),
            maximum: MAX_IDENTIFIER_LEN,
        });
    }
    if value.trim() != value {
        return Err(IdentifierError::SurroundingWhitespace);
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::ControlCharacter);
    }
    Ok(())
}

macro_rules! typed_identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Validates and constructs an identifier.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                validate_identifier(&value)?;
                Ok(Self(value))
            }

            /// Returns the identifier as an immutable string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the identifier and returns its string representation.
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

typed_identifier!(AgentId, "Stable identifier assigned to an OPPA agent.");
typed_identifier!(
    PrinterId,
    "Stable local identifier assigned to a physical or virtual printer."
);
typed_identifier!(
    PrintJobId,
    "Identifier for one requested print-job delivery."
);
typed_identifier!(
    ProductId,
    "Identifier for the product configuration compiled into an OPPA binary."
);

/// An instant represented in UTC.
///
/// The wire and database representations use RFC 3339 through `chrono`'s
/// serde implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// Returns the current UTC time.
    #[must_use]
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// Wraps a UTC `DateTime`.
    #[must_use]
    pub const fn from_datetime(value: DateTime<Utc>) -> Self {
        Self(value)
    }

    /// Returns the wrapped UTC `DateTime`.
    #[must_use]
    pub const fn as_datetime(&self) -> &DateTime<Utc> {
        &self.0
    }

    /// Consumes the timestamp.
    #[must_use]
    pub const fn into_datetime(self) -> DateTime<Utc> {
        self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            .fmt(formatter)
    }
}

/// High-level lifecycle state of the local agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    /// The binary has not been paired with a provider.
    Unconfigured,
    /// An interactive authorization flow is active.
    Authorizing,
    /// No gateway connection is active.
    Disconnected,
    /// A gateway connection is being established.
    Connecting,
    /// The gateway connection is healthy.
    Connected,
    /// The agent is operational but one or more subsystems are impaired.
    Degraded,
    /// Graceful shutdown has begun.
    ShuttingDown,
}

/// Durable local lifecycle of a print job.
///
/// `Submitted` means bytes were accepted by a spooler. It does not prove that
/// paper was physically produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    /// The validated job is durably stored.
    Received,
    /// The job is currently being handed to a spooler.
    Submitting,
    /// A spooler accepted the job.
    Submitted,
    /// Submission ended in a final failure.
    Failed,
    /// Cancellation was accepted before submission completed.
    Cancelled,
}

impl JobState {
    /// Returns whether no automatic work remains for this state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Submitted | Self::Failed | Self::Cancelled)
    }
}

/// Coarse category suitable for sanitized diagnostics and retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCategory {
    /// Input failed validation.
    Validation,
    /// A local or remote dependency could not be reached.
    Connectivity,
    /// An operation exceeded its deadline.
    Timeout,
    /// Credentials are missing, invalid, or revoked.
    Authentication,
    /// Durable state could not be read or written.
    Storage,
    /// The operation is not supported by this build or target.
    Unsupported,
    /// An operation was intentionally cancelled.
    Cancelled,
    /// A non-classified internal failure.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_ambiguous_values() {
        assert_eq!(AgentId::new(""), Err(IdentifierError::Empty));
        assert_eq!(
            PrinterId::new(" printer "),
            Err(IdentifierError::SurroundingWhitespace)
        );
        assert_eq!(
            PrintJobId::new("job\n1"),
            Err(IdentifierError::ControlCharacter)
        );
    }

    #[test]
    fn identifiers_round_trip_as_plain_json_strings() {
        let id = PrintJobId::new("job_123").expect("valid fixture id");
        let encoded = serde_json::to_string(&id).expect("serialize identifier");
        assert_eq!(encoded, "\"job_123\"");
        assert_eq!(
            serde_json::from_str::<PrintJobId>(&encoded).expect("deserialize identifier"),
            id
        );
    }

    #[test]
    fn terminal_job_states_are_explicit() {
        assert!(!JobState::Received.is_terminal());
        assert!(!JobState::Submitting.is_terminal());
        assert!(JobState::Submitted.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
    }
}
