//! Normalization of DNS-SD printer services into add-printer suggestions.
//!
//! Discovery never adds printers by itself. The user picks a suggestion,
//! which goes through the same validated manual-printer path as a typed IP
//! address and port.

// Used by the mobile host; desktop builds compile it for regression tests.
#![cfg_attr(desktop, allow(dead_code))]

use std::{collections::BTreeMap, net::IpAddr};

use serde::Serialize;

/// Service types browsed on mobile.
///
/// `_pdl-datastream._tcp` advertises the raw TCP port OPPA prints to. IPP and
/// LPD advertisements reveal a printer whose raw port is conventionally 9100.
pub const PRINTER_SERVICE_TYPES: &[&str] = &["_pdl-datastream._tcp", "_ipp._tcp", "_printer._tcp"];

/// Conventional raw ESC/POS port.
pub const RAW_TCP_PORT: u16 = 9100;

/// Endpoint reported by a platform browser before normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawServiceEndpoint {
    pub name: String,
    pub service_type: String,
    pub host: String,
    pub port: u16,
}

/// A printer suggestion shown in the add-printer flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredNetworkPrinter {
    pub name: String,
    pub host: String,
    /// Raw TCP port to print to.
    pub port: u16,
    /// Advertised DNS-SD service type.
    pub service_type: String,
    /// `true` when the port is the 9100 convention rather than advertised.
    pub assumed_port: bool,
    /// A printer with this host and port already exists.
    pub already_added: bool,
}

/// Validates, deduplicates, and ranks discovered endpoints.
///
/// One suggestion is kept per host, preferring an advertised raw port over
/// an assumed one. Names are trimmed and bounded; malformed hosts are
/// dropped.
pub fn normalize(
    endpoints: Vec<RawServiceEndpoint>,
    configured: &[(String, u16)],
) -> Vec<DiscoveredNetworkPrinter> {
    let mut by_host: BTreeMap<String, DiscoveredNetworkPrinter> = BTreeMap::new();
    for endpoint in endpoints {
        let Some(host) = valid_host(&endpoint.host) else {
            continue;
        };
        let raw = endpoint.service_type == "_pdl-datastream._tcp";
        if raw && endpoint.port == 0 {
            continue;
        }
        let port = if raw { endpoint.port } else { RAW_TCP_PORT };
        let name = bounded_name(&endpoint.name, &host);
        let candidate = DiscoveredNetworkPrinter {
            already_added: configured.iter().any(|(known, known_port)| {
                known.eq_ignore_ascii_case(&host) && *known_port == port
            }),
            name,
            host: host.clone(),
            port,
            service_type: endpoint.service_type,
            assumed_port: !raw,
        };
        match by_host.get(&host) {
            Some(existing) if !existing.assumed_port || candidate.assumed_port => {}
            _ => {
                by_host.insert(host, candidate);
            }
        }
    }
    let mut printers: Vec<_> = by_host.into_values().collect();
    printers.sort_by(|left, right| {
        left.already_added
            .cmp(&right.already_added)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    printers
}

fn valid_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.');
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address.to_string());
    }
    host.chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
        .then(|| host.to_ascii_lowercase())
}

fn bounded_name(name: &str, host: &str) -> String {
    let name: String = name
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        host.to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(name: &str, service_type: &str, host: &str, port: u16) -> RawServiceEndpoint {
        RawServiceEndpoint {
            name: name.to_owned(),
            service_type: service_type.to_owned(),
            host: host.to_owned(),
            port,
        }
    }

    #[test]
    fn advertised_raw_port_wins_over_ipp_assumption() {
        let printers = normalize(
            vec![
                endpoint("Kitchen IPP", "_ipp._tcp", "192.168.1.20", 631),
                endpoint("Kitchen", "_pdl-datastream._tcp", "192.168.1.20", 9101),
            ],
            &[],
        );
        assert_eq!(printers.len(), 1);
        assert_eq!(printers[0].port, 9101);
        assert!(!printers[0].assumed_port);
        assert_eq!(printers[0].name, "Kitchen");
    }

    #[test]
    fn ipp_only_printers_suggest_the_raw_port_convention() {
        let printers = normalize(vec![endpoint("Bar", "_ipp._tcp", "10.0.0.5", 631)], &[]);
        assert_eq!(printers[0].port, RAW_TCP_PORT);
        assert!(printers[0].assumed_port);
    }

    #[test]
    fn configured_printers_are_marked_and_sorted_last() {
        let printers = normalize(
            vec![
                endpoint("Alpha", "_pdl-datastream._tcp", "10.0.0.2", 9100),
                endpoint("Beta", "_pdl-datastream._tcp", "10.0.0.3", 9100),
            ],
            &[("10.0.0.2".to_owned(), 9100)],
        );
        assert_eq!(printers[0].name, "Beta");
        assert!(printers[1].already_added);
    }

    #[test]
    fn malformed_hosts_and_names_are_rejected_or_bounded() {
        let printers = normalize(
            vec![
                endpoint("x", "_pdl-datastream._tcp", "bad host;rm", 9100),
                endpoint("\u{0}\u{7}", "_pdl-datastream._tcp", "Printer.local.", 9100),
                endpoint("zero", "_pdl-datastream._tcp", "10.0.0.9", 0),
            ],
            &[],
        );
        assert_eq!(printers.len(), 1);
        assert_eq!(printers[0].host, "printer.local");
        assert_eq!(printers[0].name, "printer.local");
    }
}
