use std::collections::BTreeMap;

use crate::{MAX_METADATA_ENTRIES, ValidationError};

/// Opaque, bounded string metadata owned by an integrating application.
pub type Metadata = BTreeMap<String, String>;

/// Semantic validation applied after strict Serde deserialization.
pub trait Validate {
    /// Validates field bounds and cross-field invariants.
    fn validate(&self) -> Result<(), ValidationError>;
}

pub(crate) fn validate_identifier(
    value: &str,
    path: impl Into<String>,
    max: usize,
) -> Result<(), ValidationError> {
    let path = path.into();
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err(ValidationError::new(path, "must not be empty"));
    };
    let length = utf16_code_unit_count(value);
    if length > max {
        return Err(ValidationError::new(
            path,
            format!("must contain at most {max} UTF-16 code units"),
        ));
    }
    if !first.is_ascii_alphanumeric()
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
        })
    {
        return Err(ValidationError::new(
            path,
            "must use the documented identifier character set",
        ));
    }
    Ok(())
}

pub(crate) fn validate_string(
    value: &str,
    path: impl Into<String>,
    min: usize,
    max: usize,
) -> Result<(), ValidationError> {
    let path = path.into();
    let length = utf16_code_unit_count(value);
    if !(min..=max).contains(&length) {
        return Err(ValidationError::new(
            path,
            format!("must contain between {min} and {max} UTF-16 code units"),
        ));
    }
    Ok(())
}

fn utf16_code_unit_count(value: &str) -> usize {
    value.encode_utf16().count()
}

pub(crate) fn validate_timestamp(
    value: &str,
    path: impl Into<String>,
) -> Result<(), ValidationError> {
    let path = path.into();
    let bytes = value.as_bytes();
    let valid_length = bytes.len() == 20 || (22..=30).contains(&bytes.len());
    if !valid_length
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || bytes.last() != Some(&b'Z')
        || !bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| !matches!(index, 4 | 7 | 10 | 13 | 16))
            .filter(|(index, _)| *index != bytes.len() - 1)
            .all(|(index, byte)| *byte == b'.' && index == 19 || byte.is_ascii_digit())
        || bytes.len() > 20 && bytes.get(19) != Some(&b'.')
        || !valid_timestamp_ranges(bytes)
    {
        return Err(ValidationError::new(
            path,
            "must be a UTC RFC 3339 timestamp ending in Z",
        ));
    }
    Ok(())
}

fn valid_timestamp_ranges(bytes: &[u8]) -> bool {
    fn number(bytes: &[u8], start: usize) -> Option<u8> {
        let high = bytes.get(start)?.checked_sub(b'0')?;
        let low = bytes.get(start + 1)?.checked_sub(b'0')?;
        (high <= 9 && low <= 9).then_some(high * 10 + low)
    }

    matches!(number(bytes, 5), Some(1..=12))
        && matches!(number(bytes, 8), Some(1..=31))
        && matches!(number(bytes, 11), Some(0..=23))
        && matches!(number(bytes, 14), Some(0..=59))
        && matches!(number(bytes, 17), Some(0..=59))
}

pub(crate) fn validate_metadata(
    metadata: &Metadata,
    path: impl AsRef<str>,
) -> Result<(), ValidationError> {
    let path = path.as_ref();
    if metadata.len() > MAX_METADATA_ENTRIES {
        return Err(ValidationError::new(
            path,
            format!("must contain at most {MAX_METADATA_ENTRIES} entries"),
        ));
    }
    for (key, value) in metadata {
        validate_identifier(key, format!("{path}.{key}"), 64)?;
        validate_string(value, format!("{path}.{key}"), 0, 1_024)?;
    }
    Ok(())
}

// Keep the modulo form until the workspace MSRV includes `usize::is_multiple_of`.
#[allow(clippy::manual_is_multiple_of)]
pub(crate) fn validate_base64(value: &str, path: &str) -> Result<(), ValidationError> {
    if value.len() < 4 || value.len() % 4 != 0 || !value.is_ascii() {
        return Err(ValidationError::new(path, "must be padded base64"));
    }
    let padding = value.bytes().rev().take_while(|byte| *byte == b'=').count();
    if padding > 2 {
        return Err(ValidationError::new(path, "must be padded base64"));
    }
    let data_length = value.len() - padding;
    if !value
        .bytes()
        .take(data_length)
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        || !value.bytes().skip(data_length).all(|byte| byte == b'=')
    {
        return Err(ValidationError::new(path, "must be padded base64"));
    }
    Ok(())
}
