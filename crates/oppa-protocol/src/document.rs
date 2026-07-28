use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    MAX_DOCUMENT_SECTIONS, MAX_IMAGE_BASE64_LENGTH, Validate, ValidationError,
    validation::{validate_base64, validate_string},
};

/// Supported receipt media width in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceiptWidth {
    /// 58 mm receipt media.
    Mm58,
    /// 80 mm receipt media.
    Mm80,
}

impl ReceiptWidth {
    /// Returns the serialized millimetre value.
    pub const fn millimetres(self) -> u16 {
        match self {
            Self::Mm58 => 58,
            Self::Mm80 => 80,
        }
    }
}

impl Serialize for ReceiptWidth {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.millimetres())
    }
}

impl<'de> Deserialize<'de> for ReceiptWidth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WidthVisitor;

        impl<'de> de::Visitor<'de> for WidthVisitor {
            type Value = ReceiptWidth;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("receipt width 58 or 80")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    58 => Ok(ReceiptWidth::Mm58),
                    80 => Ok(ReceiptWidth::Mm80),
                    _ => Err(E::custom("receipt width must be 58 or 80")),
                }
            }
        }

        deserializer.deserialize_u64(WidthVisitor)
    }
}

/// Horizontal alignment for a text primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    /// Align to the leading edge.
    Left,
    /// Centre the text.
    Center,
    /// Align to the trailing edge.
    Right,
}

/// Supported encoded raster image media type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMediaType {
    /// Portable Network Graphics.
    #[serde(rename = "image/png")]
    Png,
    /// JPEG image data.
    #[serde(rename = "image/jpeg")]
    Jpeg,
}

/// Supported linear barcode symbology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarcodeFormat {
    /// Code 128.
    Code128,
    /// Code 39.
    Code39,
    /// EAN-13.
    Ean13,
    /// UPC-A.
    Upca,
}

/// Printer-independent structured document primitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PrintSection {
    /// Bounded Unicode text.
    Text {
        /// Text to render.
        value: String,
        /// Optional horizontal alignment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<TextAlignment>,
        /// Whether to request a bold face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bold: Option<bool>,
    },
    /// A renderer-laid-out two-column row.
    Row {
        /// Leading column.
        left: String,
        /// Trailing column.
        right: String,
    },
    /// A renderer-selected horizontal divider.
    Divider,
    /// Base64-encoded raster image data.
    Image {
        /// Explicit encoded image type.
        #[serde(rename = "mediaType")]
        media_type: ImageMediaType,
        /// Padded base64 without a data-URI prefix.
        data: String,
    },
    /// QR code content.
    Qr {
        /// Content encoded into the QR code.
        value: String,
    },
    /// Linear barcode content.
    Barcode {
        /// Explicit barcode symbology.
        format: BarcodeFormat,
        /// Printable ASCII barcode data.
        value: String,
    },
    /// Feed paper by a bounded line count.
    Feed {
        /// Number of lines to feed.
        lines: u8,
    },
    /// Ask the backend to cut using a supported cut mode.
    Cut,
}

impl Validate for PrintSection {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Text { value, .. } => validate_string(value, "value", 0, 16_384),
            Self::Row { left, right } => {
                validate_string(left, "left", 0, 4_096)?;
                validate_string(right, "right", 0, 4_096)
            }
            Self::Divider | Self::Cut => Ok(()),
            Self::Image { data, .. } => {
                validate_string(data, "data", 4, MAX_IMAGE_BASE64_LENGTH)?;
                validate_base64(data, "data")
            }
            Self::Qr { value } => validate_string(value, "value", 1, 4_096),
            Self::Barcode { value, .. } => {
                validate_string(value, "value", 1, 256)?;
                if !value.bytes().all(|byte| (b' '..=b'~').contains(&byte)) {
                    return Err(ValidationError::new(
                        "value",
                        "must contain printable ASCII only",
                    ));
                }
                Ok(())
            }
            Self::Feed { lines } if *lines == 0 => {
                Err(ValidationError::new("lines", "must be between 1 and 255"))
            }
            Self::Feed { .. } => Ok(()),
        }
    }
}

/// Printer-independent structured receipt content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintDocument {
    /// Target media width in millimetres.
    pub width: ReceiptWidth,
    /// Ordered rendering primitives.
    pub sections: Vec<PrintSection>,
}

impl Validate for PrintDocument {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.sections.is_empty() || self.sections.len() > MAX_DOCUMENT_SECTIONS {
            return Err(ValidationError::new(
                "sections",
                format!("must contain between 1 and {MAX_DOCUMENT_SECTIONS} sections"),
            ));
        }
        for (index, section) in self.sections.iter().enumerate() {
            section
                .validate()
                .map_err(|error| error.at(format!("sections.{index}")))?;
        }
        Ok(())
    }
}
