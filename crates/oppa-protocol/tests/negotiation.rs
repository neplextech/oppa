//! Compatibility tests for protocol negotiation between an agent and each server generation.
//!
//! Peers are modelled with the same codecs they use in production. A base-only server decodes with
//! `decode_*_for(&[ProtocolVersion::BASE])`, which matches the original v1 decoder. A new server
//! decodes with the default decoder, which accepts every version.

use std::fs;

use oppa_protocol::{
    AgentHello, AgentMessage, AgentMessageKind, CacheUpdate, HandshakePlanner, LegacyServerRecord,
    OpenPrinterBrandMetadata, PrinterInventory, ProtocolError, ProtocolVersion, ServerHello,
    ServerMessage, ServerMessageKind, decode_agent_message, decode_agent_message_for,
    decode_server_message_for, encode_agent_message, encode_server_message, project_inventory,
    select_protocol_version,
};

const NOW_MS: u64 = 1_800_000_000_000;
const BASE: ProtocolVersion = ProtocolVersion::V1;
const EXTENDED: ProtocolVersion = ProtocolVersion::V2;

fn fixture(path: &str) -> Vec<u8> {
    let full = format!(
        "{}/../../protocol/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::read(&full).unwrap_or_else(|error| panic!("{full} must be readable: {error}"))
}

fn agent_hello(offered: Vec<ProtocolVersion>) -> AgentMessage {
    AgentMessage {
        protocol_version: BASE,
        message_id: "msg_agent_hello_scenario".to_owned(),
        sent_at: "2026-08-01T09:00:00Z".to_owned(),
        correlation_id: None,
        kind: AgentMessageKind::Hello(AgentHello {
            agent_id: "agent_demo_001".to_owned(),
            agent_version: "1.0.0".to_owned(),
            product_id: "oppa.default".to_owned(),
            product_version: "1.0.0".to_owned(),
            supported_protocol_versions: offered,
        }),
    }
}

/// What a server replies to a hello once it has selected a version.
fn server_hello(selected: ProtocolVersion) -> ServerMessage {
    ServerMessage {
        protocol_version: BASE,
        message_id: "msg_server_hello_scenario".to_owned(),
        sent_at: "2026-08-01T09:00:01Z".to_owned(),
        correlation_id: Some("msg_agent_hello_scenario".to_owned()),
        kind: ServerMessageKind::Hello(ServerHello {
            server_id: "server_example_001".to_owned(),
            server_version: "1.0.0".to_owned(),
            brand: OpenPrinterBrandMetadata {
                name: "Scenario service".to_owned(),
            },
            session_id: "session_scenario".to_owned(),
            supported_protocol_versions: vec![selected],
            selected_protocol_version: selected,
            heartbeat_interval_ms: 30_000,
            max_message_bytes: 2_097_152,
        }),
    }
}

/// The original, base-protocol inventory fixture: a receipt printer without extended fields.
fn base_inventory() -> AgentMessage {
    decode_agent_message(&fixture("agent/printer-inventory.json")).expect("base inventory fixture")
}

/// A printer with extended document capabilities, carried in an extended-version inventory.
fn extended_inventory() -> AgentMessage {
    decode_agent_message(&fixture("agent/printer-inventory-documents.json"))
        .expect("extended inventory fixture")
}

/// Outcome of one connection attempt from the agent's perspective.
enum Attempt {
    /// The server replied with `server.hello` carrying the selected version.
    Accepted(ProtocolVersion),
    /// The server closed the connection before replying.
    ClosedBeforeHello,
}

/// A base-only server: decodes with the original v1 rules and closes on any rejected hello.
fn base_only_server(hello_bytes: &[u8]) -> Attempt {
    match decode_agent_message_for(hello_bytes, &[BASE]) {
        Ok(hello) => {
            let AgentMessageKind::Hello(hello) = hello.kind else {
                panic!("the hello fixture must be a hello");
            };
            let selected = select_protocol_version(&[BASE], &hello.supported_protocol_versions)
                .expect("base-only server accepts a base offer");
            Attempt::Accepted(selected)
        }
        Err(_) => Attempt::ClosedBeforeHello,
    }
}

/// A new server: accepts every version this crate implements.
fn new_server(hello_bytes: &[u8]) -> Attempt {
    let hello = decode_agent_message(hello_bytes).expect("new server decodes every valid hello");
    let AgentMessageKind::Hello(hello) = hello.kind else {
        panic!("the hello must be a hello");
    };
    let selected = select_protocol_version(
        ProtocolVersion::SUPPORTED,
        &hello.supported_protocol_versions,
    )
    .expect("new server shares a version with every agent");
    Attempt::Accepted(selected)
}

fn encode_hello(message: &AgentMessage) -> Vec<u8> {
    encode_agent_message(message).expect("agent hello encodes")
}

/// Runs the agent's connection loop against a server, following the planner's fallback rule.
fn connect(
    planner: &mut HandshakePlanner,
    server: impl Fn(&[u8]) -> Attempt,
) -> (ProtocolVersion, Vec<Attempt>) {
    let mut attempts = Vec::new();
    loop {
        let hello = encode_hello(&agent_hello(planner.offered().to_vec()));
        let outcome = server(&hello);
        let closed = matches!(outcome, Attempt::ClosedBeforeHello);
        attempts.push(outcome);
        if closed && planner.on_closed_before_hello() {
            continue;
        }
        let Some(Attempt::Accepted(selected)) = attempts.last() else {
            panic!("the connection ended and no fallback applies");
        };
        return (*selected, attempts);
    }
}

#[test]
fn old_agent_and_new_server_negotiate_the_base_protocol_with_the_original_handshake() {
    // An original agent offers only the base protocol, exactly as its fixture does.
    let hello = fixture("agent/agent-hello.json");
    let Attempt::Accepted(selected) = new_server(&hello) else {
        panic!("a new server accepts an original agent");
    };
    assert_eq!(selected, BASE);

    // The reply lists only the selected version, which base-only agents can decode.
    let reply = encode_server_message(&server_hello(selected)).expect("server hello encodes");
    let decoded = decode_server_message_for(&reply, &[BASE]).expect("base agent decodes the reply");
    let ServerMessageKind::Hello(decoded) = decoded.kind else {
        panic!("reply must be a hello");
    };
    assert_eq!(decoded.supported_protocol_versions, vec![BASE]);
}

#[test]
fn old_agent_inventory_stays_byte_compatible_with_the_base_protocol() {
    let inventory = base_inventory();
    assert_eq!(inventory.protocol_version, BASE);
    let encoded = encode_agent_message(&inventory).expect("base inventory encodes");
    let decoded_by_base_server = decode_agent_message_for(&encoded, &[BASE])
        .expect("a base-only server accepts the original inventory");
    assert_eq!(decoded_by_base_server, inventory);
}

#[test]
fn new_agent_and_new_server_negotiate_the_extended_protocol_and_exchange_document_capabilities() {
    let mut planner = HandshakePlanner::new(None, NOW_MS);
    let (selected, attempts) = connect(&mut planner, new_server);
    assert_eq!(selected, EXTENDED);
    assert_eq!(attempts.len(), 1, "no fallback is needed");

    // Extended inventory is labelled with the extended version and accepted by the new server.
    let inventory = extended_inventory();
    assert_eq!(inventory.protocol_version, EXTENDED);
    let encoded = encode_agent_message(&inventory).expect("extended inventory encodes");
    let decoded = decode_agent_message(&encoded).expect("new server accepts extended inventory");
    assert_eq!(decoded, inventory);

    // The same inventory cannot be labelled with the base version, because it needs the extension.
    let mut mislabelled = inventory.clone();
    mislabelled.protocol_version = BASE;
    assert!(matches!(
        encode_agent_message(&mislabelled),
        Err(ProtocolError::InvalidMessage(_))
    ));
}

#[test]
fn new_agent_with_a_base_only_server_falls_back_once_and_then_completes_the_base_handshake() {
    let mut planner = HandshakePlanner::new(None, NOW_MS);
    let (selected, attempts) = connect(&mut planner, base_only_server);

    assert_eq!(selected, BASE);
    assert!(
        matches!(attempts[0], Attempt::ClosedBeforeHello),
        "the extended offer is refused"
    );
    assert!(
        matches!(attempts[1], Attempt::Accepted(BASE)),
        "the base retry completes"
    );

    // The agent remembers the server for the next connections.
    assert_eq!(
        planner.on_accepted(selected, NOW_MS),
        CacheUpdate::Store(LegacyServerRecord {
            legacy_until_ms: NOW_MS + oppa_protocol::LEGACY_RETRY_INTERVAL_MS,
        })
    );
}

#[test]
fn a_remembered_base_only_server_is_not_probed_again_until_the_interval_expires() {
    let record = LegacyServerRecord {
        legacy_until_ms: NOW_MS + 60_000,
    };
    let mut planner = HandshakePlanner::new(Some(record), NOW_MS);
    let (selected, attempts) = connect(&mut planner, base_only_server);
    assert_eq!(selected, BASE);
    assert_eq!(
        attempts.len(),
        1,
        "no extended attempt is sent to a remembered server"
    );

    let mut after_interval = HandshakePlanner::new(Some(record), NOW_MS + 61_000);
    let (_, retried) = connect(&mut after_interval, base_only_server);
    assert!(
        matches!(retried[0], Attempt::ClosedBeforeHello),
        "the extended offer is tried again"
    );
}

#[test]
fn a_base_only_session_never_carries_extended_fields() {
    let printers = match extended_inventory().kind {
        AgentMessageKind::PrinterInventory(PrinterInventory { printers, .. }) => printers,
        other => panic!("unexpected inventory kind {other:?}"),
    };
    let projected = project_inventory(&printers, BASE);
    let inventory = AgentMessage {
        protocol_version: BASE,
        message_id: "msg_projected_inventory".to_owned(),
        sent_at: "2026-08-01T09:02:00Z".to_owned(),
        correlation_id: None,
        kind: AgentMessageKind::PrinterInventory(PrinterInventory {
            revision: 8,
            printers: projected,
        }),
    };

    let encoded = encode_agent_message(&inventory).expect("projected inventory encodes");
    let text = String::from_utf8(encoded.clone()).expect("UTF-8");
    for field in [
        "documentFormats",
        "paperSizes",
        "customPaperSize",
        "printOptions",
    ] {
        assert!(
            !text.contains(field),
            "projected inventory must not contain {field}"
        );
    }
    decode_agent_message_for(&encoded, &[BASE]).expect("base-only server accepts the projection");
}

#[test]
fn a_base_only_server_rejects_extended_content_even_under_an_extended_envelope() {
    let encoded = encode_agent_message(&extended_inventory()).expect("extended inventory encodes");
    assert!(matches!(
        decode_agent_message_for(&encoded, &[BASE]),
        Err(ProtocolError::UnsupportedProtocolVersion { .. })
    ));
}

#[test]
fn servers_cannot_negotiate_a_version_the_agent_does_not_offer() {
    let selected = select_protocol_version(ProtocolVersion::SUPPORTED, &[BASE]);
    assert_eq!(selected, Some(BASE));
    assert_eq!(select_protocol_version(&[EXTENDED], &[BASE]), None);
}

#[test]
fn new_server_replies_are_rejected_by_a_base_agent_when_they_select_the_extended_version() {
    // Only a version the agent offered may be selected. A base-only agent never offers the extended version.
    let reply = encode_server_message(&server_hello(EXTENDED)).expect("extended reply encodes");
    assert!(decode_server_message_for(&reply, &[BASE]).is_err());
}

#[test]
fn the_planner_and_cache_update_agree_with_the_selection_rule() {
    let mut planner = HandshakePlanner::new(None, NOW_MS);
    let (selected, _) = connect(&mut planner, new_server);
    assert_eq!(
        planner.on_accepted(selected, NOW_MS),
        CacheUpdate::Unchanged
    );
}

#[test]
fn an_extended_hello_round_trips_through_the_default_decoder() {
    let hello = agent_hello(vec![EXTENDED, BASE]);
    let encoded = encode_hello(&hello);
    assert_eq!(
        decode_agent_message(&encoded).expect("hello decodes"),
        hello
    );
}
