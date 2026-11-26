//! Direct-download Android update discovery and verification.
//!
//! The desktop build uses the signed Tauri updater. Android builds are
//! distributed as signed APKs (published on GitHub Releases and served by the
//! Neplex update server), so the flow is different:
//!
//! 1. fetch the Android release manifest over HTTPS; HTTP 204 means no update;
//! 2. offer the update only when its version is newer than the installed app;
//! 3. download the APK over HTTPS (following HTTPS-only redirects) into the
//!    private update cache with a size bound and an explicit timeout,
//!    verifying its SHA-256 digest;
//! 4. hand it to the system installer, which (after a native check) only
//!    accepts an APK with this app's package name and signing certificate and
//!    always asks the user to approve installation.
//!
//! No Google Play or Play services dependency is involved.

// Used by the mobile host; desktop builds compile it for regression tests.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Upper bound for a downloaded APK.
pub const MAX_APK_BYTES: u64 = 256 * 1024 * 1024;
/// Upper bound for the update manifest.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Maximum redirects followed for a manifest or APK request. Every hop must
/// also use HTTPS.
pub const MAX_REDIRECTS: usize = 10;

/// Release manifest published next to the APK assets.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AndroidUpdateManifest {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    pub android: AndroidAsset,
}

/// One universal or ABI-specific APK.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AndroidAsset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Update offer shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileUpdate {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub size_bytes: u64,
}

/// Validation failures for a manifest or download.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("the update manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("the downloaded update did not match its published checksum")]
    ChecksumMismatch,
    #[error("the downloaded update exceeded its published size")]
    SizeExceeded,
    #[error("the update server responded with HTTP {0}")]
    UnexpectedStatus(u16),
}

/// Whether a manifest response with `status` carries a body worth reading.
///
/// HTTP 204 means the server has no newer release for this installation.
/// Any other non-2xx status is an error.
pub fn manifest_status_has_body(status: u16) -> Result<bool, UpdateError> {
    match status {
        204 => Ok(false),
        200..=299 => Ok(true),
        _ => Err(UpdateError::UnexpectedStatus(status)),
    }
}

/// Interprets a manifest response, returning the manifest and an offer when
/// a newer, valid release is available.
pub fn interpret_manifest_response(
    status: u16,
    body: &[u8],
    current_version: &str,
) -> Result<Option<(AndroidUpdateManifest, MobileUpdate)>, UpdateError> {
    if !manifest_status_has_body(status)? {
        return Ok(None);
    }
    let manifest = AndroidUpdateManifest::parse(body)?;
    Ok(manifest
        .offer(current_version)
        .map(|offer| (manifest, offer)))
}

impl AndroidUpdateManifest {
    /// Parses and validates a manifest.
    pub fn parse(bytes: &[u8]) -> Result<Self, UpdateError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(UpdateError::InvalidManifest(
                "manifest is too large".to_owned(),
            ));
        }
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| UpdateError::InvalidManifest(error.to_string()))?;
        semver::Version::parse(manifest.version.trim_start_matches('v'))
            .map_err(|_| UpdateError::InvalidManifest("version is not semantic".to_owned()))?;
        let url = url::Url::parse(&manifest.android.url)
            .map_err(|_| UpdateError::InvalidManifest("APK URL is invalid".to_owned()))?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err(UpdateError::InvalidManifest(
                "APK URL must use HTTPS without credentials".to_owned(),
            ));
        }
        if manifest.android.sha256.len() != 64
            || !manifest
                .android
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(UpdateError::InvalidManifest(
                "APK checksum must be a SHA-256 hex digest".to_owned(),
            ));
        }
        if manifest.android.size == 0 || manifest.android.size > MAX_APK_BYTES {
            return Err(UpdateError::InvalidManifest(
                "APK size is out of range".to_owned(),
            ));
        }
        Ok(manifest)
    }

    /// Returns an offer when the manifest is newer than `current_version`.
    pub fn offer(&self, current_version: &str) -> Option<MobileUpdate> {
        let current = semver::Version::parse(current_version.trim_start_matches('v')).ok()?;
        let available = semver::Version::parse(self.version.trim_start_matches('v')).ok()?;
        (available > current).then(|| MobileUpdate {
            current_version: current.to_string(),
            version: available.to_string(),
            notes: self.notes.clone(),
            published_at: self.pub_date.clone(),
            size_bytes: self.android.size,
        })
    }
}

/// Incremental size- and digest-checked download sink.
pub struct VerifiedDownload {
    hasher: Sha256,
    expected_sha256: String,
    expected_size: u64,
    received: u64,
}

impl VerifiedDownload {
    pub fn new(asset: &AndroidAsset) -> Self {
        Self {
            hasher: Sha256::new(),
            expected_sha256: asset.sha256.to_ascii_lowercase(),
            expected_size: asset.size,
            received: 0,
        }
    }

    /// Accounts for one chunk; rejects data beyond the published size.
    pub fn update(&mut self, chunk: &[u8]) -> Result<(), UpdateError> {
        self.received = self.received.saturating_add(chunk.len() as u64);
        if self.received > self.expected_size {
            return Err(UpdateError::SizeExceeded);
        }
        self.hasher.update(chunk);
        Ok(())
    }

    /// Verifies the final size and digest.
    pub fn finish(self) -> Result<(), UpdateError> {
        if self.received != self.expected_size {
            return Err(UpdateError::ChecksumMismatch);
        }
        let digest = self.hasher.finalize();
        let actual = digest
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        if actual == self.expected_sha256 {
            Ok(())
        } else {
            Err(UpdateError::ChecksumMismatch)
        }
    }
}

/// Network side of the Android update flow.
#[cfg(target_os = "android")]
pub mod android {
    use std::{path::PathBuf, time::Duration};

    use tokio::io::AsyncWriteExt as _;

    use super::{
        AndroidUpdateManifest, MAX_MANIFEST_BYTES, MAX_REDIRECTS, MobileUpdate, VerifiedDownload,
        interpret_manifest_response, manifest_status_has_body,
    };
    use crate::error::CommandError;

    /// Release manifest location, fixed at compile time for release builds.
    /// Debug builds never check for updates, matching the desktop dev config.
    pub const MANIFEST_URL: Option<&str> = if cfg!(debug_assertions) {
        None
    } else {
        match option_env!("OPPA_ANDROID_UPDATE_MANIFEST") {
            Some(url) => Some(url),
            None => Some("https://updates.neplex.com.np/oppa/stable/android/latest.json"),
        }
    };

    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(20);
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

    fn client(timeout: Duration) -> Result<reqwest::Client, CommandError> {
        reqwest::Client::builder()
            .https_only(true)
            // Release assets are served through redirects; reqwest applies
            // `https_only` to every hop, so a downgrade is refused.
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .connect_timeout(Duration::from_secs(15))
            .timeout(timeout)
            .build()
            .map_err(|error| CommandError::internal(error.to_string()))
    }

    /// Fetches the manifest and returns it with an offer when newer.
    pub async fn check(
        current_version: &str,
    ) -> Result<Option<(AndroidUpdateManifest, MobileUpdate)>, CommandError> {
        let Some(url) = MANIFEST_URL else {
            return Ok(None);
        };
        let mut response = client(MANIFEST_TIMEOUT)?
            .get(url)
            .send()
            .await
            .map_err(|error| CommandError::new("update_check_failed", error.to_string()))?;
        let status = response.status().as_u16();
        let check_failed =
            |error: super::UpdateError| CommandError::new("update_check_failed", error.to_string());
        if !manifest_status_has_body(status).map_err(check_failed)? {
            return Ok(None);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| CommandError::new("update_check_failed", error.to_string()))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_MANIFEST_BYTES {
                return Err(CommandError::new(
                    "update_check_failed",
                    "The update manifest is too large.",
                ));
            }
        }
        interpret_manifest_response(status, &body, current_version).map_err(check_failed)
    }

    /// Downloads and verifies the APK into `cache_dir/updates`.
    pub async fn download(
        manifest: &AndroidUpdateManifest,
        cache_dir: &std::path::Path,
    ) -> Result<PathBuf, CommandError> {
        let directory = cache_dir.join("updates");
        // Only the newest verified download is ever kept.
        let _ = tokio::fs::remove_dir_all(&directory).await;
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let path = directory.join(format!("oppa-{}.apk", manifest.version));
        let partial = directory.join("download.partial");

        let mut response = client(DOWNLOAD_TIMEOUT)?
            .get(&manifest.android.url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| CommandError::new("update_download_failed", error.to_string()))?;
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let mut verifier = VerifiedDownload::new(&manifest.android);
        let result =
            async {
                while let Some(chunk) = response.chunk().await.map_err(|error| {
                    CommandError::new("update_download_failed", error.to_string())
                })? {
                    verifier.update(&chunk).map_err(|error| {
                        CommandError::new("update_verification_failed", error.to_string())
                    })?;
                    file.write_all(&chunk)
                        .await
                        .map_err(|error| CommandError::internal(error.to_string()))?;
                }
                file.flush()
                    .await
                    .map_err(|error| CommandError::internal(error.to_string()))?;
                verifier.finish().map_err(|error| {
                    CommandError::new("update_verification_failed", error.to_string())
                })
            }
            .await;
        drop(file);
        if let Err(error) = result {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(error);
        }
        tokio::fs::rename(&partial, &path)
            .await
            .map_err(|error| CommandError::internal(error.to_string()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str, url: &str, sha: &str, size: u64) -> Vec<u8> {
        format!(
            r#"{{"version":"{version}","notes":"Fixes","pub_date":"2026-11-01T00:00:00Z","android":{{"url":"{url}","sha256":"{sha}","size":{size}}}}}"#
        )
        .into_bytes()
    }

    const SHA_OF_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn newer_versions_are_offered_and_older_ones_are_not() {
        let parsed = AndroidUpdateManifest::parse(&manifest(
            "0.4.0",
            "https://github.com/neplextech/oppa/releases/download/oppa-v0.4.0/oppa.apk",
            SHA_OF_ABC,
            3,
        ))
        .expect("manifest");
        let offer = parsed.offer("0.3.1").expect("offer");
        assert_eq!(offer.version, "0.4.0");
        assert_eq!(offer.current_version, "0.3.1");
        assert!(parsed.offer("0.4.0").is_none());
        assert!(parsed.offer("1.0.0").is_none());
    }

    #[test]
    fn insecure_or_malformed_manifests_are_rejected() {
        for bytes in [
            manifest("0.4.0", "http://example.com/a.apk", SHA_OF_ABC, 3),
            manifest("0.4.0", "https://user:pw@example.com/a.apk", SHA_OF_ABC, 3),
            manifest("latest", "https://example.com/a.apk", SHA_OF_ABC, 3),
            manifest("0.4.0", "https://example.com/a.apk", "abc", 3),
            manifest("0.4.0", "https://example.com/a.apk", SHA_OF_ABC, 0),
            manifest(
                "0.4.0",
                "https://example.com/a.apk",
                SHA_OF_ABC,
                MAX_APK_BYTES + 1,
            ),
        ] {
            assert!(AndroidUpdateManifest::parse(&bytes).is_err());
        }
    }

    #[test]
    fn no_content_means_no_update_without_parsing_a_body() {
        assert_eq!(interpret_manifest_response(204, b"", "0.4.0"), Ok(None));
        // A 204 is never parsed, even if an intermediary attached bytes.
        assert_eq!(
            interpret_manifest_response(204, b"garbage", "0.4.0"),
            Ok(None)
        );
    }

    #[test]
    fn successful_responses_are_parsed_and_compared() {
        let body = manifest(
            "0.5.0",
            "https://updates.neplex.com.np/oppa/stable/android/oppa.apk",
            SHA_OF_ABC,
            3,
        );
        let (parsed, offer) = interpret_manifest_response(200, &body, "0.4.0")
            .expect("valid")
            .expect("offer");
        assert_eq!(parsed.version, "0.5.0");
        assert_eq!(offer.current_version, "0.4.0");
        assert_eq!(interpret_manifest_response(200, &body, "0.5.0"), Ok(None));
    }

    #[test]
    fn unavailable_servers_and_malformed_bodies_are_errors() {
        for status in [404, 500, 502, 503] {
            assert_eq!(
                interpret_manifest_response(status, b"<html>Site not available</html>", "0.4.0"),
                Err(UpdateError::UnexpectedStatus(status))
            );
        }
        for body in [
            &b""[..],
            b"<!DOCTYPE html>",
            b"{}",
            b"{\"version\":\"0.5.0\"}",
        ] {
            assert!(matches!(
                interpret_manifest_response(200, body, "0.4.0"),
                Err(UpdateError::InvalidManifest(_))
            ));
        }
    }

    #[test]
    fn desktop_updater_uses_the_dynamic_stable_endpoint_and_dev_disables_it() {
        let release: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("release config");
        let updater = &release["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://updates.neplex.com.np/oppa/stable/tauri/{{target}}/{{arch}}/{{current_version}}"
            ])
        );
        assert!(
            updater["pubkey"]
                .as_str()
                .is_some_and(|key| !key.is_empty())
        );

        let dev: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.dev.json")).expect("dev config");
        assert_eq!(
            dev["plugins"]["updater"]["endpoints"],
            serde_json::json!([])
        );
    }

    #[test]
    fn download_verification_checks_digest_and_size() {
        let asset = AndroidAsset {
            url: "https://example.com/a.apk".to_owned(),
            sha256: SHA_OF_ABC.to_owned(),
            size: 3,
        };
        let mut ok = VerifiedDownload::new(&asset);
        ok.update(b"ab").expect("chunk");
        ok.update(b"c").expect("chunk");
        assert!(ok.finish().is_ok());

        let mut tampered = VerifiedDownload::new(&asset);
        tampered.update(b"abd").expect("chunk");
        assert_eq!(tampered.finish(), Err(UpdateError::ChecksumMismatch));

        let mut oversized = VerifiedDownload::new(&asset);
        assert_eq!(oversized.update(b"abcd"), Err(UpdateError::SizeExceeded));

        let mut truncated = VerifiedDownload::new(&asset);
        truncated.update(b"ab").expect("chunk");
        assert_eq!(truncated.finish(), Err(UpdateError::ChecksumMismatch));
    }
}
