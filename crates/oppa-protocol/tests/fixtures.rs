use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use oppa_protocol::{
    AgentMessageKind, MAX_WIRE_MESSAGE_BYTES, ProtocolError, ProtocolMessage, Validate,
    decode_agent_message, decode_protocol_message, decode_server_message, encode_agent_message,
    encode_server_message,
};
use serde_json::Value;

const AGENT_TYPES: &[&str] = &[
    "agent.hello",
    "agent.authentication_metadata",
    "agent.heartbeat",
    "agent.printer_inventory",
    "agent.printer_inventory_changed",
    "agent.job_received",
    "agent.job_submitted",
    "agent.job_failed",
    "agent.diagnostics",
];

const SERVER_TYPES: &[&str] = &[
    "server.hello",
    "server.heartbeat",
    "server.print_job",
    "server.cancel_job",
    "server.request_printer_inventory",
    "server.configuration_invalidated",
    "server.disconnect",
];

fn fixture_directory(group: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/fixtures")
        .join(group)
}

fn fixture_files(group: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(fixture_directory(group))
        .expect("shared fixture directory must exist")
        .map(|entry| entry.expect("fixture entry must be readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn every_agent_fixture_validates_and_round_trips() {
    let mut observed = BTreeSet::new();
    for path in fixture_files("agent") {
        let raw = fs::read(&path).expect("agent fixture must be readable");
        let message = decode_agent_message(&raw)
            .unwrap_or_else(|error| panic!("{} must validate: {error}", path.display()));
        observed.insert(message.message_type());
        message
            .validate()
            .unwrap_or_else(|error| panic!("{} must validate: {error}", path.display()));

        let encoded = encode_agent_message(&message)
            .unwrap_or_else(|error| panic!("{} must encode: {error}", path.display()));
        let expected: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
        let actual: Value = serde_json::from_slice(&encoded).expect("encoding must be JSON");
        assert_eq!(actual, expected, "fixture {}", path.display());
    }
    assert_eq!(observed, AGENT_TYPES.iter().copied().collect());
}

#[test]
fn every_server_fixture_validates_and_round_trips() {
    let mut observed = BTreeSet::new();
    for path in fixture_files("server") {
        let raw = fs::read(&path).expect("server fixture must be readable");
        let message = decode_server_message(&raw)
            .unwrap_or_else(|error| panic!("{} must validate: {error}", path.display()));
        observed.insert(message.message_type());
        message
            .validate()
            .unwrap_or_else(|error| panic!("{} must validate: {error}", path.display()));

        let encoded = encode_server_message(&message)
            .unwrap_or_else(|error| panic!("{} must encode: {error}", path.display()));
        let expected: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
        let actual: Value = serde_json::from_slice(&encoded).expect("encoding must be JSON");
        assert_eq!(actual, expected, "fixture {}", path.display());
    }
    assert_eq!(observed, SERVER_TYPES.iter().copied().collect());
}

#[test]
fn every_invalid_fixture_is_rejected() {
    for path in fixture_files("invalid") {
        let raw = fs::read(&path).expect("invalid fixture must be readable");
        let result = decode_protocol_message(&raw);
        assert!(result.is_err(), "{} must be rejected", path.display());

        if path.ends_with("unsupported-version.json") {
            assert!(
                matches!(
                    result,
                    Err(ProtocolError::UnsupportedProtocolVersion {
                        received,
                        supported: &[1],
                    }) if received == "99"
                ),
                "{} must produce a version error",
                path.display(),
            );
        } else {
            assert!(
                matches!(result, Err(ProtocolError::InvalidMessage(_))),
                "{} must produce a structural validation error",
                path.display(),
            );
        }
    }
}

#[test]
fn generic_decoder_routes_both_directions() {
    let agent = fs::read(fixture_directory("agent").join("agent-hello.json"))
        .expect("agent fixture must be readable");
    let server = fs::read(fixture_directory("server").join("server-hello.json"))
        .expect("server fixture must be readable");

    assert!(matches!(
        decode_protocol_message(&agent),
        Ok(ProtocolMessage::Agent(_))
    ));
    assert!(matches!(
        decode_protocol_message(&server),
        Ok(ProtocolMessage::Server(_))
    ));
}

#[test]
fn codecs_reject_unknown_fields_and_oversized_messages() {
    let raw = fs::read(fixture_directory("server").join("heartbeat.json"))
        .expect("fixture must be readable");
    let mut value: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
    value
        .as_object_mut()
        .expect("fixture must be an object")
        .insert("unexpected".to_owned(), Value::Bool(true));

    assert!(matches!(
        decode_server_message(
            &serde_json::to_vec(&value).expect("modified fixture must serialize")
        ),
        Err(ProtocolError::InvalidMessage(_))
    ));
    assert!(matches!(
        decode_protocol_message(&vec![b'x'; MAX_WIRE_MESSAGE_BYTES + 1]),
        Err(ProtocolError::MessageTooLarge { .. })
    ));
}

#[test]
fn malformed_image_data_is_rejected() {
    let raw = fs::read(fixture_directory("server").join("print-job.json"))
        .expect("fixture must be readable");
    let mut value: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
    value["payload"]["document"]["sections"][3]["data"] =
        Value::String("/tmp/receipt.png".to_owned());

    assert!(matches!(
        decode_server_message(
            &serde_json::to_vec(&value).expect("modified fixture must serialize")
        ),
        Err(ProtocolError::InvalidMessage(_))
    ));
}

#[test]
fn string_bounds_match_javascript_utf16_code_units() {
    let raw = fs::read(fixture_directory("server").join("print-job.json"))
        .expect("fixture must be readable");
    let mut value: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
    let at_limit = "😀".repeat(128);
    assert_eq!(at_limit.encode_utf16().count(), 256);
    value["payload"]["idempotencyKey"] = Value::String(at_limit.clone());

    decode_server_message(&serde_json::to_vec(&value).expect("modified fixture must serialize"))
        .expect("256 UTF-16 code units must satisfy maxLength 256");

    value["payload"]["idempotencyKey"] = Value::String(format!("{at_limit}a"));
    assert!(matches!(
        decode_server_message(
            &serde_json::to_vec(&value).expect("modified fixture must serialize")
        ),
        Err(ProtocolError::InvalidMessage(_))
    ));
}

#[test]
fn printer_capabilities_may_be_absent_when_discovery_cannot_determine_them() {
    let raw =
        fs::read(fixture_directory("agent").join("printer-inventory-unknown-capabilities.json"))
            .expect("fixture must be readable");
    let value: Value = serde_json::from_slice(&raw).expect("fixture must be JSON");
    let message =
        decode_agent_message(&raw).expect("unknown capabilities may be represented by absence");
    let AgentMessageKind::PrinterInventory(inventory) = &message.kind else {
        panic!("fixture must be a printer inventory");
    };
    assert_eq!(inventory.printers.len(), 1);
    assert!(inventory.printers[0].capabilities.is_none());
    let round_trip = encode_agent_message(&message).expect("message must re-encode");
    assert_eq!(
        serde_json::from_slice::<Value>(&round_trip).expect("round trip must be JSON"),
        value
    );
}

#[test]
fn json_schema_integer_notation_is_accepted() {
    let raw = fs::read_to_string(fixture_directory("server").join("heartbeat.json"))
        .expect("fixture must be readable")
        .replace("\"protocolVersion\": 1", "\"protocolVersion\": 1.0")
        .replace("\"timeoutMs\": 15000", "\"timeoutMs\": 15000.0");

    decode_server_message(raw.as_bytes())
        .expect("mathematically integral JSON numbers must satisfy integer schemas");
}
