//! CUPS queue option discovery for PDF capability reporting.
//!
//! `lpstat` names queues but does not say which paper sizes or duplex modes a
//! driver offers. `lpoptions -p <queue> -l` does, in `Key/Label: *Default
//! value value` lines for both PPD and driverless IPP queues. Parsing is kept
//! separate from process execution so it can be tested with captured output.

#![cfg_attr(not(unix), allow(dead_code))]

use std::time::Duration;

use oppa_printer::{PdfCapabilities, StandardPaper};
use tokio::process::Command;

use crate::{DiscoveryError, DiscoveryResult};

/// Media and duplex options reported by one CUPS queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CupsQueueOptions {
    /// Standard paper sizes the queue offers, in a stable order.
    pub paper_sizes: Vec<StandardPaper>,
    /// Whether the queue offers a custom media size.
    pub custom_paper: bool,
    /// Whether the queue offers two-sided printing.
    pub duplex: bool,
}

impl CupsQueueOptions {
    /// Converts parsed options into the PDF capability advertised to the server.
    #[must_use]
    pub fn pdf_capabilities(&self) -> PdfCapabilities {
        PdfCapabilities {
            native: true,
            duplex: self.duplex,
            custom_paper: self.custom_paper,
            paper_sizes: self.paper_sizes.clone(),
        }
    }
}

/// Parses `lpoptions -l` output into media and duplex options.
///
/// Unknown keys are ignored. A duplex option counts only when it names a mode
/// other than `None` or `one-sided`.
#[must_use]
pub fn parse_cups_options(output: &str) -> CupsQueueOptions {
    let mut options = CupsQueueOptions::default();
    for line in output.lines() {
        let Some((key, values)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let tokens: Vec<&str> = values
            .split_whitespace()
            .map(|token| token.trim_start_matches('*'))
            .collect();
        if key.starts_with("pagesize") || key.starts_with("media/") || key == "media" {
            for token in &tokens {
                if let Some(size) = standard_paper(token) {
                    if !options.paper_sizes.contains(&size) {
                        options.paper_sizes.push(size);
                    }
                } else if token.to_ascii_lowercase().starts_with("custom") {
                    options.custom_paper = true;
                }
            }
        } else if key.contains("duplex") || key.starts_with("sides") {
            options.duplex |= tokens.iter().any(|token| {
                let token = token.to_ascii_lowercase();
                token.starts_with("duplex") || token.starts_with("two-sided")
            });
        }
    }
    options.paper_sizes.sort_by_key(|size| match size {
        StandardPaper::A4 => 0,
        StandardPaper::A5 => 1,
        StandardPaper::Letter => 2,
        StandardPaper::Legal => 3,
    });
    options
}

fn standard_paper(token: &str) -> Option<StandardPaper> {
    match token.to_ascii_lowercase().as_str() {
        "a4" | "iso_a4_210x297mm" => Some(StandardPaper::A4),
        "a5" | "iso_a5_148x210mm" => Some(StandardPaper::A5),
        "letter" | "na_letter_8.5x11in" => Some(StandardPaper::Letter),
        "legal" | "na_legal_8.5x14in" => Some(StandardPaper::Legal),
        _ => None,
    }
}

/// Queries one queue's options with a hard deadline.
///
/// # Errors
///
/// Returns an error when the query cannot start, exceeds its deadline, or
/// exits unsuccessfully. Callers should then report the queue without media
/// details rather than guess them.
pub async fn query_cups_options(
    queue: &str,
    deadline: Duration,
) -> DiscoveryResult<CupsQueueOptions> {
    let mut command = Command::new("lpoptions");
    command
        .args(["-p", queue, "-l"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(deadline, command.output())
        .await
        .map_err(|_| DiscoveryError::Timeout(deadline))?
        .map_err(|error| DiscoveryError::Unavailable(format!("cannot run lpoptions: {error}")))?;
    if !output.status.success() {
        return Err(DiscoveryError::Unavailable(format!(
            "lpoptions exited with {}",
            output.status
        )));
    }
    let text = String::from_utf8(output.stdout).map_err(|error| {
        DiscoveryError::InvalidData(format!("lpoptions returned non-UTF-8 output: {error}"))
    })?;
    Ok(parse_cups_options(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppd_queue_reports_media_custom_and_duplex_options() {
        let options = parse_cups_options(
            "PageSize/Media Size: *A4 A5 Letter Legal Custom.WIDTHxHEIGHT\n\
             Duplex/Double-Sided Printing: *None DuplexNoTumble DuplexTumble\n\
             ColorModel/Color: *Gray RGB\n",
        );
        assert_eq!(
            options.paper_sizes,
            vec![
                StandardPaper::A4,
                StandardPaper::A5,
                StandardPaper::Letter,
                StandardPaper::Legal
            ]
        );
        assert!(options.custom_paper);
        assert!(options.duplex);
    }

    #[test]
    fn driverless_ipp_queue_reports_pwg_media_and_sides() {
        let options = parse_cups_options(
            "media/Media Size: *iso_a4_210x297mm na_letter_8.5x11in\n\
             sides/Two-Sided: *one-sided two-sided-long-edge two-sided-short-edge\n",
        );
        assert_eq!(
            options.paper_sizes,
            vec![StandardPaper::A4, StandardPaper::Letter]
        );
        assert!(!options.custom_paper);
        assert!(options.duplex);
    }

    #[test]
    fn single_sided_only_queues_do_not_claim_duplex() {
        let options = parse_cups_options(
            "Duplex/Double-Sided Printing: *None\n\
             sides/Two-Sided: *one-sided\n",
        );
        assert!(!options.duplex);
    }

    #[test]
    fn unknown_output_yields_no_claims() {
        let options = parse_cups_options("Resolution/Output: *300dpi 600dpi\nnonsense line\n");
        assert_eq!(options, CupsQueueOptions::default());
        assert_eq!(
            options.pdf_capabilities(),
            PdfCapabilities {
                native: true,
                duplex: false,
                custom_paper: false,
                paper_sizes: Vec::new(),
            }
        );
    }
}
