//! Protocol version negotiation rules shared by agents, servers, and conformance tests.
//!
//! An extended agent offers `[V2, V1]`. A base-only server cannot read that hello, so it closes the
//! connection without replying. The agent then retries once with `[V1]` on a fresh connection. When
//! that retry succeeds, the agent remembers that the server is base-only for
//! [`LEGACY_RETRY_INTERVAL_MS`], so later connections skip the failed attempt. After the interval
//! the agent offers the extended protocol again, so a server upgrade is picked up without re-pairing.
//!
//! The planner never infers support from application or server version strings. It uses only the
//! explicit outcome of each hello exchange.

use crate::{PrinterDescriptor, ProtocolVersion};

/// How long an agent keeps offering only the base protocol to a server that rejected the extended offer.
pub const LEGACY_RETRY_INTERVAL_MS: u64 = 24 * 60 * 60 * 1_000;

/// Chooses the highest version both peers support.
///
/// The choice is made from the server's own list, so a peer cannot select a version the server did
/// not enable. Returns `None` when the lists share no version.
#[must_use]
pub fn select_protocol_version(
    server_supported: &[ProtocolVersion],
    offered: &[ProtocolVersion],
) -> Option<ProtocolVersion> {
    [ProtocolVersion::EXTENDED, ProtocolVersion::BASE]
        .into_iter()
        .find(|version| server_supported.contains(version) && offered.contains(version))
}

/// Returns the inventory a session that negotiated `version` may carry.
///
/// Base-only sessions receive descriptors without extended capabilities, so the peer never sees
/// fields it cannot decode.
#[must_use]
pub fn project_inventory(
    printers: &[PrinterDescriptor],
    version: ProtocolVersion,
) -> Vec<PrinterDescriptor> {
    printers
        .iter()
        .map(|printer| match version {
            ProtocolVersion::V1 => printer.without_extended_capabilities(),
            ProtocolVersion::V2 => printer.clone(),
        })
        .collect()
}

/// The agent's stored knowledge that one server accepts only the base protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyServerRecord {
    /// Unix time in milliseconds after which the extended offer is tried again.
    pub legacy_until_ms: u64,
}

/// What the host should persist after a completed hello exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheUpdate {
    /// Keep the stored state as it is.
    Unchanged,
    /// Remember that this server accepted only the base protocol.
    Store(LegacyServerRecord),
    /// Forget any stored base-only record because the server accepted the extended protocol.
    Clear,
}

/// Plans the hello offers for one agent across connection attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakePlanner {
    offered: Vec<ProtocolVersion>,
    had_record: bool,
    may_fall_back: bool,
    fell_back: bool,
}

impl HandshakePlanner {
    /// Creates the plan for the first connection attempt.
    ///
    /// `record` is the stored base-only record for this server, if any. While it is current, the
    /// agent offers only the base protocol and does not attempt the extended protocol.
    #[must_use]
    pub fn new(record: Option<LegacyServerRecord>, now_ms: u64) -> Self {
        let legacy_is_current = record.is_some_and(|record| record.legacy_until_ms > now_ms);
        if legacy_is_current {
            Self {
                offered: vec![ProtocolVersion::BASE],
                had_record: true,
                may_fall_back: false,
                fell_back: false,
            }
        } else {
            Self {
                offered: vec![ProtocolVersion::EXTENDED, ProtocolVersion::BASE],
                had_record: record.is_some(),
                may_fall_back: true,
                fell_back: false,
            }
        }
    }

    /// Versions the next hello should offer.
    #[must_use]
    pub fn offered(&self) -> &[ProtocolVersion] {
        &self.offered
    }

    /// Records that a connection ended after the hello was sent and before `server.hello`.
    ///
    /// Returns `true` when the caller should retry immediately with [`Self::offered`], which is the
    /// base-only offer. Returns `false` when no fallback applies, and normal reconnect backoff should run.
    /// A timeout waiting for `server.hello` is not a rejection and must not call this method.
    pub fn on_closed_before_hello(&mut self) -> bool {
        if !self.may_fall_back || self.fell_back {
            return false;
        }
        self.offered = vec![ProtocolVersion::BASE];
        self.fell_back = true;
        true
    }

    /// Records the version selected by `server.hello` and returns what to persist.
    #[must_use]
    pub fn on_accepted(&self, selected: ProtocolVersion, now_ms: u64) -> CacheUpdate {
        match selected {
            ProtocolVersion::BASE if self.fell_back => CacheUpdate::Store(LegacyServerRecord {
                legacy_until_ms: now_ms.saturating_add(LEGACY_RETRY_INTERVAL_MS),
            }),
            ProtocolVersion::EXTENDED if self.had_record => CacheUpdate::Clear,
            _ => CacheUpdate::Unchanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PrinterAvailability, PrinterCapabilities, PrinterConnection, PrinterKind};

    const NOW: u64 = 1_800_000_000_000;

    fn printer(extended: bool) -> PrinterDescriptor {
        PrinterDescriptor {
            id: "printer_office_001".to_owned(),
            fingerprint: "system:CUPS:office".to_owned(),
            name: "Office".to_owned(),
            kind: PrinterKind::Local,
            connection: PrinterConnection::System {
                system_name: "office".to_owned(),
            },
            capabilities: Some(PrinterCapabilities {
                media_widths: vec![crate::ReceiptWidth::Mm80],
                system_driver: Some(true),
                esc_pos: None,
                raster: true,
                cut: false,
                qr: true,
                barcode: true,
                document_formats: extended.then(|| vec![crate::DocumentFormat::Receipt]),
                paper_sizes: None,
                custom_paper_size: None,
                print_options: None,
            }),
            enabled: true,
            availability: PrinterAvailability::Online,
        }
    }

    #[test]
    fn selection_prefers_the_extended_version_only_when_both_peers_have_it() {
        use ProtocolVersion::{V1 as BASE, V2 as EXTENDED};
        assert_eq!(
            select_protocol_version(&[BASE, EXTENDED], &[EXTENDED, BASE]),
            Some(EXTENDED)
        );
        assert_eq!(
            select_protocol_version(&[BASE], &[EXTENDED, BASE]),
            Some(BASE)
        );
        assert_eq!(
            select_protocol_version(&[BASE, EXTENDED], &[BASE]),
            Some(BASE)
        );
        assert_eq!(select_protocol_version(&[EXTENDED], &[BASE]), None);
    }

    #[test]
    fn projection_removes_extended_fields_only_for_base_sessions() {
        let printers = vec![printer(true)];
        let base = project_inventory(&printers, ProtocolVersion::BASE);
        assert!(
            base[0]
                .capabilities
                .as_ref()
                .unwrap()
                .document_formats
                .is_none()
        );
        let extended = project_inventory(&printers, ProtocolVersion::EXTENDED);
        assert!(
            extended[0]
                .capabilities
                .as_ref()
                .unwrap()
                .document_formats
                .is_some()
        );
    }

    #[test]
    fn a_fresh_agent_offers_the_extended_protocol_first() {
        let planner = HandshakePlanner::new(None, NOW);
        assert_eq!(
            planner.offered(),
            &[ProtocolVersion::EXTENDED, ProtocolVersion::BASE]
        );
    }

    #[test]
    fn a_rejected_extended_offer_falls_back_once_and_is_remembered() {
        let mut planner = HandshakePlanner::new(None, NOW);
        assert!(
            planner.on_closed_before_hello(),
            "the first rejection retries at once"
        );
        assert_eq!(planner.offered(), &[ProtocolVersion::BASE]);
        assert!(
            !planner.on_closed_before_hello(),
            "a second failure uses normal backoff"
        );

        assert_eq!(
            planner.on_accepted(ProtocolVersion::BASE, NOW),
            CacheUpdate::Store(LegacyServerRecord {
                legacy_until_ms: NOW + LEGACY_RETRY_INTERVAL_MS,
            })
        );
    }

    #[test]
    fn a_base_handshake_without_a_fallback_stores_nothing() {
        let planner = HandshakePlanner::new(None, NOW);
        assert_eq!(
            planner.on_accepted(ProtocolVersion::BASE, NOW),
            CacheUpdate::Unchanged
        );
    }

    #[test]
    fn a_current_legacy_record_skips_the_extended_offer() {
        let record = LegacyServerRecord {
            legacy_until_ms: NOW + 1_000,
        };
        let mut planner = HandshakePlanner::new(Some(record), NOW);
        assert_eq!(planner.offered(), &[ProtocolVersion::BASE]);
        assert!(
            !planner.on_closed_before_hello(),
            "no extended attempt means no fallback"
        );
    }

    #[test]
    fn an_expired_legacy_record_retries_the_extended_protocol() {
        let record = LegacyServerRecord {
            legacy_until_ms: NOW - 1,
        };
        let planner = HandshakePlanner::new(Some(record), NOW);
        assert_eq!(
            planner.offered(),
            &[ProtocolVersion::EXTENDED, ProtocolVersion::BASE]
        );
        assert_eq!(
            planner.on_accepted(ProtocolVersion::EXTENDED, NOW),
            CacheUpdate::Clear
        );
    }
}
