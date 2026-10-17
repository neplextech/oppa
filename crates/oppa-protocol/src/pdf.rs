use serde::{Deserialize, Serialize};

use crate::{
    MAX_PDF_DOCUMENT_BYTES, MAX_PDF_URL_BYTES, Validate, ValidationError,
    validation::{validate_sha256, validate_string},
};

/// Minimum copies accepted for one job.
pub const MIN_COPIES: u16 = 1;
/// Maximum copies accepted for one job.
pub const MAX_COPIES: u16 = 99;
/// Minimum custom paper dimension in millimetres, sized for small labels.
pub const MIN_CUSTOM_PAPER_MM: u16 = 10;
/// Maximum custom paper dimension in millimetres.
pub const MAX_CUSTOM_PAPER_MM: u16 = 1_000;
/// Minimum explicit scale percentage.
pub const MIN_SCALE_PERCENT: u16 = 10;
/// Maximum explicit scale percentage.
pub const MAX_SCALE_PERCENT: u16 = 400;
/// Maximum UTF-8 length of a page-range expression.
pub const MAX_PAGE_RANGE_BYTES: usize = 256;

/// The only PDF document representation understood by protocol version 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfFormat {
    /// Portable Document Format.
    #[serde(rename = "pdf")]
    Pdf,
}

/// Where the agent retrieves a PDF from, with integrity metadata.
///
/// Bytes never travel inside protocol messages. The agent fetches the URL
/// with its own authenticated request, verifies the byte length and SHA-256
/// digest, and then renders the verified bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PdfSource {
    /// Authenticated HTTPS retrieval; plain HTTP is accepted only on loopback.
    Url {
        /// Absolute retrieval URL without credentials or a fragment.
        url: String,
        /// Lowercase hexadecimal SHA-256 digest of the exact PDF bytes.
        sha256: String,
        /// Exact number of PDF bytes the server will serve.
        byte_length: u64,
    },
}

impl Validate for PdfSource {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Url {
                url,
                sha256,
                byte_length,
            } => {
                validate_document_url(url, "url")?;
                validate_sha256(sha256, "sha256")?;
                if *byte_length == 0 || *byte_length > MAX_PDF_DOCUMENT_BYTES {
                    return Err(ValidationError::new(
                        "byteLength",
                        format!("must be between 1 and {MAX_PDF_DOCUMENT_BYTES} bytes"),
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Standard paper names with reliable meaning in every backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandardPaperSize {
    /// ISO A4, 210 × 297 mm.
    A4,
    /// ISO A5, 148 × 210 mm.
    A5,
    /// North American Letter, 8.5 × 11 in.
    Letter,
    /// North American Legal, 8.5 × 14 in.
    Legal,
}

/// Requested paper size: a standard name or explicit millimetre dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaperSize {
    /// A standard named size.
    Standard(StandardPaperSize),
    /// Custom media, such as a label stock, in millimetres.
    Custom(CustomPaperSize),
}

/// Custom media dimensions in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomPaperSize {
    /// Width in millimetres.
    pub width_mm: u16,
    /// Height in millimetres.
    pub height_mm: u16,
}

impl Validate for PaperSize {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Standard(_) => Ok(()),
            Self::Custom(custom) => {
                validate_custom_mm(custom.width_mm, "widthMm")?;
                validate_custom_mm(custom.height_mm, "heightMm")
            }
        }
    }
}

fn validate_custom_mm(value: u16, path: &str) -> Result<(), ValidationError> {
    if !(MIN_CUSTOM_PAPER_MM..=MAX_CUSTOM_PAPER_MM).contains(&value) {
        return Err(ValidationError::new(
            path,
            format!("must be between {MIN_CUSTOM_PAPER_MM} and {MAX_CUSTOM_PAPER_MM} millimetres"),
        ));
    }
    Ok(())
}

/// Page orientation applied before physical placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    /// Height is the longer edge.
    Portrait,
    /// Width is the longer edge.
    Landscape,
}

/// How a page is scaled to the selected paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scaling {
    /// Scale content down (never up) so the whole page fits the paper.
    Fit,
    /// Print at 100% without scaling.
    Actual,
    /// Print at an explicit percentage, serialized as `{"percent": 80}`.
    Percent(u16),
}

impl Validate for Scaling {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Fit | Self::Actual => Ok(()),
            Self::Percent(percent) => {
                if (MIN_SCALE_PERCENT..=MAX_SCALE_PERCENT).contains(percent) {
                    Ok(())
                } else {
                    Err(ValidationError::new(
                        "percent",
                        format!(
                            "must be between {MIN_SCALE_PERCENT} and {MAX_SCALE_PERCENT} percent"
                        ),
                    ))
                }
            }
        }
    }
}

/// Requested duplex mode. Backends that cannot honour it must fail the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duplex {
    /// Single-sided printing.
    #[serde(rename = "simplex")]
    Simplex,
    /// Two-sided printing flipped on the long edge.
    #[serde(rename = "long-edge")]
    LongEdge,
    /// Two-sided printing flipped on the short edge.
    #[serde(rename = "short-edge")]
    ShortEdge,
}

/// Print settings for a document. Every omitted field uses its documented default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintSettings {
    /// Number of copies, from 1 to 99. Defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copies: Option<u16>,
    /// Target paper. Defaults to each page's own media size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper: Option<PaperSize>,
    /// Page orientation. Defaults to the document's own orientation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    /// Scaling mode. Defaults to `actual`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// Page selection such as `1-3,5`. Defaults to every page, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_ranges: Option<String>,
    /// Duplex mode. Defaults to single-sided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplex: Option<Duplex>,
}

impl PrintSettings {
    /// Returns the effective copy count.
    #[must_use]
    pub fn copies(&self) -> u16 {
        self.copies.unwrap_or(MIN_COPIES)
    }

    /// Returns the effective duplex mode.
    #[must_use]
    pub fn duplex(&self) -> Duplex {
        self.duplex.unwrap_or(Duplex::Simplex)
    }
}

impl Validate for PrintSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(copies) = self.copies
            && !(MIN_COPIES..=MAX_COPIES).contains(&copies)
        {
            return Err(ValidationError::new(
                "copies",
                format!("must be between {MIN_COPIES} and {MAX_COPIES}"),
            ));
        }
        if let Some(paper) = &self.paper {
            paper.validate().map_err(|error| error.at("paper"))?;
        }
        if let Some(scaling) = &self.scaling {
            scaling.validate().map_err(|error| error.at("scaling"))?;
        }
        if let Some(ranges) = &self.page_ranges {
            validate_page_ranges(ranges)?;
        }
        Ok(())
    }
}

/// Validates the syntax of a page-range expression such as `1-3,5`.
///
/// Page-count bounds are checked by the renderer, which knows the document.
///
/// # Errors
///
/// Returns a [`ValidationError`] when the expression is empty, too long,
/// malformed, or contains a descending or zero-based range.
pub fn validate_page_ranges(value: &str) -> Result<(), ValidationError> {
    if value.is_empty() || value.len() > MAX_PAGE_RANGE_BYTES {
        return Err(ValidationError::new(
            "pageRanges",
            format!("must contain between 1 and {MAX_PAGE_RANGE_BYTES} bytes"),
        ));
    }
    for part in value.split(',') {
        let (start, end) = match part.split_once('-') {
            Some((start, end)) => (start, end),
            None => (part, part),
        };
        let start = parse_page_number(start)?;
        let end = parse_page_number(end)?;
        if start > end {
            return Err(ValidationError::new(
                "pageRanges",
                "ranges must ascend from the first page to the last page",
            ));
        }
    }
    Ok(())
}

fn parse_page_number(value: &str) -> Result<u32, ValidationError> {
    let malformed = || {
        ValidationError::new(
            "pageRanges",
            "must contain comma-separated page numbers or ascending dash ranges",
        )
    };
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed());
    }
    let number: u32 = value.parse().map_err(|_| malformed())?;
    if number == 0 {
        return Err(ValidationError::new(
            "pageRanges",
            "page numbers start at 1",
        ));
    }
    Ok(number)
}

/// A PDF document reference plus print settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfDocument {
    /// Discriminator selecting the PDF representation.
    pub format: PdfFormat,
    /// Authenticated, integrity-checked retrieval reference.
    pub source: PdfSource,
    /// Optional print settings; absent means every default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<PrintSettings>,
}

impl Validate for PdfDocument {
    fn validate(&self) -> Result<(), ValidationError> {
        self.source.validate().map_err(|error| error.at("source"))?;
        if let Some(settings) = &self.settings {
            settings.validate().map_err(|error| error.at("settings"))?;
        }
        Ok(())
    }
}

/// Validates a PDF retrieval URL.
///
/// Production URLs must use HTTPS. Plain HTTP is accepted only for loopback
/// development hosts. Credentials, fragments, whitespace, and IPv6 literals
/// are rejected so the value cannot smuggle credentials or confuse logs.
///
/// # Errors
///
/// Returns a [`ValidationError`] at `path` when the URL is not allowed.
pub fn validate_document_url(value: &str, path: &str) -> Result<(), ValidationError> {
    if value.len() > MAX_PDF_URL_BYTES {
        return Err(ValidationError::new(
            path,
            format!("must contain at most {MAX_PDF_URL_BYTES} bytes"),
        ));
    }
    let rest = if let Some(rest) = value.strip_prefix("https://") {
        rest
    } else if let Some(rest) = value.strip_prefix("http://") {
        let authority = rest.split('/').next().unwrap_or_default();
        let host = authority.split(':').next().unwrap_or_default();
        if host != "localhost" && host != "127.0.0.1" {
            return Err(ValidationError::new(
                path,
                "plain HTTP is allowed only for loopback hosts",
            ));
        }
        rest
    } else {
        return Err(ValidationError::new(path, "must be an absolute HTTPS URL"));
    };
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let host_valid = !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
    let port_valid = port.is_none_or(|port| {
        !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !host_valid || !port_valid {
        return Err(ValidationError::new(
            path,
            "must contain a plain host name or IPv4 address without credentials",
        ));
    }
    if value
        .bytes()
        .any(|byte| byte <= b' ' || byte == 0x7f || byte == b'#')
    {
        return Err(ValidationError::new(
            path,
            "must not contain whitespace, control characters, or a fragment",
        ));
    }
    validate_string(value, path, 1, MAX_PDF_URL_BYTES)
}
