use serde::Serialize;

/// Stable, serializable failure returned across the Tauri command boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    message: String,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl AsRef<str>) -> Self {
        Self {
            code,
            message: sanitize(message.as_ref()),
        }
    }

    pub fn internal(message: impl AsRef<str>) -> Self {
        Self::new("internal", message)
    }

    pub fn not_found(entity: &'static str) -> Self {
        Self::new("not_found", format!("{entity} was not found"))
    }

    pub fn invalid(message: impl AsRef<str>) -> Self {
        Self::new("invalid_input", message)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

pub fn sanitize(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .take(1_000)
        .collect::<String>();
    for marker in ["privateKey", "pairingCode", "challengePayload", "signature"] {
        redact_after_marker(&mut sanitized, marker);
    }
    if sanitized.trim().is_empty() {
        "Operation failed without diagnostic details.".to_owned()
    } else {
        sanitized
    }
}

fn redact_after_marker(value: &mut String, marker: &str) {
    let mut search_from = 0;
    while let Some(relative) = value[search_from..].find(marker) {
        let start = search_from + relative + marker.len();
        let end = value[start..]
            .find(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '&' | ',' | ';' | '"' | '\'')
            })
            .map_or(value.len(), |relative_end| start + relative_end);
        value.replace_range(start..end, "[REDACTED]");
        search_from = start + "[REDACTED]".len();
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn redacts_current_credential_markers_and_bounds_output() {
        let message = format!(
            "privateKey=secret-key signature=secret-signature {}",
            "x".repeat(2_000)
        );
        let sanitized = sanitize(&message);

        assert!(!sanitized.contains("secret-key"));
        assert!(!sanitized.contains("secret-signature"));
        assert!(sanitized.len() <= 1_050);
    }
}
