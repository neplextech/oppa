//! Compile-time product configuration for OPPA distributions.
//!
//! `build.rs` reads `OPPA_PRODUCT_DIR/product.json` (defaulting to
//! `products/default`), validates its schema and feature compatibility, and
//! embeds both the JSON and regular files below `assets/`. Runtime edits to the
//! source product directory therefore cannot change a built binary.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod schema;

use std::{path::Path, sync::OnceLock};

pub use schema::{
    PRODUCT_SCHEMA_VERSION, ProductBranding, ProductConfig, ProductFeatures, ProductLegal,
    ProductProtocol, ProductUpdates,
};
use thiserror::Error;

/// A product asset embedded in the binary at compile time.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedAsset {
    /// Forward-slash-separated path relative to the product `assets/` directory.
    pub path: &'static str,
    /// Immutable contents captured by the build.
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_product.rs"));

static PRODUCT: OnceLock<Result<ProductConfig, String>> = OnceLock::new();

/// Errors produced while loading a product definition outside the build script.
#[derive(Debug, Error)]
pub enum ProductConfigError {
    /// The file could not be read.
    #[error("cannot read product configuration at {path}: {source}")]
    Read {
        /// Attempted file path.
        path: String,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// JSON did not match the versioned schema.
    #[error("product configuration is not valid JSON schema v1: {0}")]
    Parse(#[from] serde_json::Error),
    /// One or more semantic requirements were violated.
    #[error("product configuration failed validation: {0}")]
    Validation(String),
    /// Build-generated configuration was unexpectedly unreadable.
    #[error("embedded product configuration is invalid: {0}")]
    Embedded(String),
}

/// Returns the validated product configuration embedded in this binary.
///
/// The build script validates the same representation before compilation. The
/// `Result` keeps corruption or future generator drift explicit instead of
/// introducing a runtime panic.
pub fn embedded_product() -> Result<&'static ProductConfig, ProductConfigError> {
    PRODUCT
        .get_or_init(|| {
            serde_json::from_str::<ProductConfig>(EMBEDDED_PRODUCT_JSON)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let issues = config.validation_issues();
                    if issues.is_empty() {
                        Ok(config)
                    } else {
                        Err(issues.join("; "))
                    }
                })
        })
        .as_ref()
        .map_err(|error| ProductConfigError::Embedded(error.clone()))
}

/// Returns every asset embedded from the configured product directory.
#[must_use]
pub const fn embedded_assets() -> &'static [EmbeddedAsset] {
    EMBEDDED_ASSETS
}

/// Finds an embedded asset by its normalized relative path.
#[must_use]
pub fn embedded_asset(path: &str) -> Option<&'static EmbeddedAsset> {
    let normalized = path.trim_start_matches('/').replace('\\', "/");
    EMBEDDED_ASSETS
        .iter()
        .find(|asset| asset.path == normalized)
}

/// Parses and validates a product file, primarily for tooling and tests.
///
/// Production binaries should use [`embedded_product`] so configuration cannot
/// be altered after compilation.
pub fn load_product_file(path: impl AsRef<Path>) -> Result<ProductConfig, ProductConfigError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path).map_err(|source| ProductConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let product = serde_json::from_str::<ProductConfig>(&source)?;
    let issues = product.validation_issues();
    if issues.is_empty() {
        Ok(product)
    } else {
        Err(ProductConfigError::Validation(issues.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn valid_json() -> &'static str {
        r#"{
          "schemaVersion": 1,
          "productId": "test",
          "productName": "Test Agent",
          "applicationId": "com.example.test",
          "description": "Test product",
          "protocol": {
            "clientId": "test-agent",
            "authorizationUrl": "http://127.0.0.1:3000/authorize",
            "tokenUrl": "http://localhost:3000/token",
            "gatewayUrl": "ws://127.0.0.1:3000/openprinter"
          },
          "updates": { "endpoint": null },
          "branding": {
            "supportUrl": "https://example.com/support",
            "documentationUrl": "https://example.com/docs"
          },
          "features": {}
        }"#
    }

    #[test]
    fn accepts_tls_and_loopback_development_endpoints() {
        let product: ProductConfig = serde_json::from_str(valid_json()).expect("valid schema");
        assert!(product.validation_issues().is_empty());
    }

    #[test]
    fn rejects_insecure_remote_and_unknown_fields() {
        let insecure = valid_json().replace(
            "https://example.com/support",
            "http://remote.example/support",
        );
        let product: ProductConfig = serde_json::from_str(&insecure).expect("valid JSON shape");
        assert!(
            product
                .validation_issues()
                .iter()
                .any(|issue| issue.contains("branding.supportUrl"))
        );

        let unknown = valid_json().replace(
            "\"description\": \"Test product\",",
            "\"description\": \"Test product\", \"surprise\": true,",
        );
        assert!(serde_json::from_str::<ProductConfig>(&unknown).is_err());
    }

    #[test]
    fn file_loader_reports_all_semantic_errors() {
        let mut file = tempfile::NamedTempFile::new().expect("temporary product");
        file.write_all(
            valid_json()
                .replace("\"Test Agent\"", "\" \"")
                .replace("\"com.example.test\"", "\"invalid\"")
                .replace("\"test-agent\"", "\" \"")
                .as_bytes(),
        )
        .expect("write fixture");
        let error = load_product_file(file.path()).expect_err("invalid product");
        let message = error.to_string();
        assert!(message.contains("productName"));
        assert!(message.contains("applicationId"));
        assert!(message.contains("protocol.clientId"));
    }

    #[test]
    fn embedded_product_was_validated_by_build() {
        let product = embedded_product().expect("build validated embedded product");
        assert_eq!(product.schema_version, PRODUCT_SCHEMA_VERSION);
        assert!(embedded_assets().iter().all(|asset| {
            !asset.path.starts_with('/') && !asset.path.split('/').any(|segment| segment == "..")
        }));
    }
}
