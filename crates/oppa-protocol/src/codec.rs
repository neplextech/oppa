use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    AgentMessage, MAX_WIRE_MESSAGE_BYTES, PROTOCOL_VERSION, ProtocolError, ProtocolMessage,
    ServerMessage, Validate, ValidationError,
};

const SUPPORTED_VERSIONS: &[u16] = &[PROTOCOL_VERSION];
const JSON_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const ENVELOPE_FIELDS: &[&str] = &[
    "protocolVersion",
    "messageId",
    "sentAt",
    "correlationId",
    "type",
    "payload",
];

/// Decodes UTF-8 JSON and validates an agent-to-server message.
pub fn decode_agent_message(input: &[u8]) -> Result<AgentMessage, ProtocolError> {
    let value = decode_value(input)?;
    validate_envelope_shape(&value)?;
    let message: AgentMessage = deserialize_message(value)?;
    message.validate()?;
    Ok(message)
}

/// Decodes UTF-8 JSON and validates a server-to-agent message.
pub fn decode_server_message(input: &[u8]) -> Result<ServerMessage, ProtocolError> {
    let value = decode_value(input)?;
    validate_envelope_shape(&value)?;
    let message: ServerMessage = deserialize_message(value)?;
    message.validate()?;
    Ok(message)
}

/// Decodes UTF-8 JSON and routes a validated message by its discriminator.
pub fn decode_protocol_message(input: &[u8]) -> Result<ProtocolMessage, ProtocolError> {
    let value = decode_value(input)?;
    validate_envelope_shape(&value)?;
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| ValidationError::new("type", "must be a string discriminator"))?;

    if message_type.starts_with("agent.") {
        let message: AgentMessage = deserialize_message(value)?;
        message.validate()?;
        Ok(ProtocolMessage::Agent(message))
    } else if message_type.starts_with("server.") {
        let message: ServerMessage = deserialize_message(value)?;
        message.validate()?;
        Ok(ProtocolMessage::Server(message))
    } else {
        Err(ValidationError::new("type", "is not a documented message discriminator").into())
    }
}

/// Validates and encodes an agent message as compact UTF-8 JSON.
pub fn encode_agent_message(message: &AgentMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_message(message)
}

/// Validates and encodes a server message as compact UTF-8 JSON.
pub fn encode_server_message(message: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_message(message)
}

/// Validates and encodes either protocol direction as compact UTF-8 JSON.
pub fn encode_protocol_message(message: &ProtocolMessage) -> Result<Vec<u8>, ProtocolError> {
    match message {
        ProtocolMessage::Agent(message) => encode_agent_message(message),
        ProtocolMessage::Server(message) => encode_server_message(message),
    }
}

fn decode_value(input: &[u8]) -> Result<Value, ProtocolError> {
    enforce_size(input.len())?;
    let mut value: Value = serde_json::from_slice(input).map_err(|_| ProtocolError::InvalidJson)?;
    normalize_integral_numbers(&mut value);
    assert_supported_version(&value)?;
    Ok(value)
}

fn normalize_integral_numbers(value: &mut Value) {
    match value {
        Value::Number(number) if number.as_i64().is_none() && number.as_u64().is_none() => {
            let Some(float) = number.as_f64() else {
                return;
            };
            if !float.is_finite() || float.fract() != 0.0 {
                return;
            }
            if (0.0..=JSON_SAFE_INTEGER).contains(&float) {
                *value = Value::from(float as u64);
            } else if (-JSON_SAFE_INTEGER..0.0).contains(&float) {
                *value = Value::from(float as i64);
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_integral_numbers(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_integral_numbers(value);
            }
        }
        _ => {}
    }
}

fn assert_supported_version(value: &Value) -> Result<(), ProtocolError> {
    let Some(version) = value.get("protocolVersion") else {
        return Ok(());
    };
    if version == &Value::from(PROTOCOL_VERSION) {
        return Ok(());
    }

    let received = match version {
        Value::Null => Some("null".to_owned()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
    };
    if let Some(received) = received {
        return Err(ProtocolError::UnsupportedProtocolVersion {
            received,
            supported: SUPPORTED_VERSIONS,
        });
    }
    Ok(())
}

fn validate_envelope_shape(value: &Value) -> Result<(), ProtocolError> {
    let object = value
        .as_object()
        .ok_or_else(|| ValidationError::new("/", "message must be a JSON object"))?;
    reject_unknown_fields(object, ENVELOPE_FIELDS, "/")
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    path: &str,
) -> Result<(), ProtocolError> {
    if let Some(field) = object.keys().find(|field| {
        !allowed
            .iter()
            .any(|allowed_field| allowed_field == &field.as_str())
    }) {
        return Err(ValidationError::new(path, format!("contains unknown field {field}")).into());
    }
    Ok(())
}

fn deserialize_message<T>(value: Value) -> Result<T, ProtocolError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| {
        ProtocolError::InvalidMessage(ValidationError::new("/", error.to_string()))
    })
}

fn encode_message<T>(message: &T) -> Result<Vec<u8>, ProtocolError>
where
    T: Validate + Serialize,
{
    message.validate()?;
    let encoded =
        serde_json::to_vec(message).map_err(|error| ProtocolError::Encoding(error.to_string()))?;
    enforce_size(encoded.len())?;
    Ok(encoded)
}

fn enforce_size(actual: usize) -> Result<(), ProtocolError> {
    if actual > MAX_WIRE_MESSAGE_BYTES {
        return Err(ProtocolError::MessageTooLarge {
            actual,
            limit: MAX_WIRE_MESSAGE_BYTES,
        });
    }
    Ok(())
}
