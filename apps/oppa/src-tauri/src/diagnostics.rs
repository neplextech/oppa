use std::{collections::VecDeque, sync::Mutex};

use chrono::Utc;

use crate::{
    error::sanitize,
    models::{DiagnosticLevel, DiagnosticLogEntry},
};

const MAX_LOG_ENTRIES: usize = 200;

/// Bounded in-memory operational log used by the local diagnostics screen.
pub struct DiagnosticLog {
    entries: Mutex<VecDeque<DiagnosticLogEntry>>,
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)),
        }
    }
}

impl DiagnosticLog {
    pub fn info(&self, target: impl AsRef<str>, message: impl AsRef<str>) {
        self.push(DiagnosticLevel::Info, target.as_ref(), message.as_ref());
    }

    pub fn warn(&self, target: impl AsRef<str>, message: impl AsRef<str>) {
        self.push(DiagnosticLevel::Warn, target.as_ref(), message.as_ref());
    }

    pub fn error(&self, target: impl AsRef<str>, message: impl AsRef<str>) {
        self.push(DiagnosticLevel::Error, target.as_ref(), message.as_ref());
    }

    pub fn entries(&self) -> Vec<DiagnosticLogEntry> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn push(&self, level: DiagnosticLevel, target: &str, message: &str) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while entries.len() >= MAX_LOG_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(DiagnosticLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            level,
            target: sanitize(target),
            message: sanitize(message),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticLog, MAX_LOG_ENTRIES};

    #[test]
    fn retains_only_the_newest_bounded_entries() {
        let log = DiagnosticLog::default();
        for index in 0..(MAX_LOG_ENTRIES + 5) {
            log.info("test", format!("entry-{index}"));
        }

        let entries = log.entries();
        assert_eq!(entries.len(), MAX_LOG_ENTRIES);
        assert_eq!(entries[0].message, "entry-5");
    }
}
