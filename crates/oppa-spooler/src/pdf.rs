//! PDF submission for operating-system print queues.
//!
//! Unix hosts hand the validated PDF to CUPS with `lp`, which applies the
//! driver's own PDF filters. Settings are translated to IPP attributes, so the
//! document is never flattened unless CUPS itself has to. Windows has no
//! native PDF submission path in this crate, so it rasterizes the selected
//! pages and uses the existing GDI page backend.

use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

#[cfg(unix)]
use std::{ffi::OsString, path::Path, process::Stdio};

#[cfg(unix)]
use oppa_protocol::{Duplex, PaperSize, PrintSettings, Scaling, StandardPaperSize};
use oppa_renderer::PdfPrintDocument;
#[cfg(windows)]
use oppa_renderer::{PdfRasterOptions, RasterDocument, rasterize_pdf};
#[cfg(unix)]
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
use crate::{SpoolerError, SpoolerResult, SubmissionReceipt, lp_receipt};
#[cfg(windows)]
use crate::{SpoolerError, SpoolerResult, SubmissionReceipt, windows_print_driver};

/// Resolution used when a PDF page must be rasterized for a system driver.
#[cfg(windows)]
const DRIVER_PDF_DPI: u16 = 300;

/// Rasterizes a validated PDF into device pages for a driver submission.
///
/// The deadline limits the time spent between pages; a page already in flight
/// is allowed to finish.
#[cfg(windows)]
fn rasterize_for_driver(
    document: &PdfPrintDocument,
    deadline: Duration,
) -> SpoolerResult<RasterDocument> {
    rasterize_pdf(
        document,
        PdfRasterOptions {
            dpi: DRIVER_PDF_DPI,
            deadline: Some(Instant::now() + deadline),
        },
    )
    .map_err(|error| match error {
        oppa_renderer::RendererError::RasterizationTimedOut => SpoolerError::Timeout {
            stage: "PDF rasterization",
            duration: deadline,
        },
        other => SpoolerError::DocumentRender(other.to_string()),
    })
}

/// Builds the `lp` argument vector for a native PDF job.
///
/// Every setting maps to an IPP attribute. Absent paper keeps the driver's
/// default media; absent scaling is pinned to actual size so output does not
/// depend on the driver's automatic fit behaviour.
#[cfg(unix)]
pub(crate) fn cups_pdf_args(
    queue_name: &str,
    settings: &PrintSettings,
    path: &Path,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-d"),
        OsString::from(queue_name),
        OsString::from("-n"),
        OsString::from(settings.copies().to_string()),
    ];
    if let Some(ranges) = &settings.page_ranges {
        args.push(OsString::from("-P"));
        args.push(OsString::from(ranges));
    }
    if let Some(paper) = settings.paper {
        push_option(&mut args, "media", &cups_media_name(paper));
    }
    if let Some(orientation) = settings.orientation {
        let value = match orientation {
            oppa_protocol::Orientation::Portrait => "3",
            oppa_protocol::Orientation::Landscape => "4",
        };
        push_option(&mut args, "orientation-requested", value);
    }
    push_option(&mut args, "sides", cups_sides(settings.duplex()));
    match settings.scaling {
        Some(Scaling::Percent(percent)) => {
            push_option(&mut args, "print-scaling", "none");
            push_option(&mut args, "scaling", &percent.to_string());
        }
        Some(Scaling::Fit) => push_option(&mut args, "print-scaling", "fit"),
        Some(Scaling::Actual) | None => push_option(&mut args, "print-scaling", "none"),
    }
    args.push(path.as_os_str().to_owned());
    args
}

#[cfg(unix)]
fn push_option(args: &mut Vec<OsString>, name: &str, value: &str) {
    args.push(OsString::from("-o"));
    args.push(OsString::from(format!("{name}={value}")));
}

#[cfg(unix)]
fn cups_sides(duplex: Duplex) -> &'static str {
    match duplex {
        Duplex::Simplex => "one-sided",
        Duplex::LongEdge => "two-sided-long-edge",
        Duplex::ShortEdge => "two-sided-short-edge",
    }
}

#[cfg(unix)]
fn cups_media_name(paper: PaperSize) -> String {
    match paper {
        PaperSize::Standard(StandardPaperSize::A4) => "A4".to_owned(),
        PaperSize::Standard(StandardPaperSize::A5) => "A5".to_owned(),
        PaperSize::Standard(StandardPaperSize::Letter) => "Letter".to_owned(),
        PaperSize::Standard(StandardPaperSize::Legal) => "Legal".to_owned(),
        PaperSize::Custom(custom) => {
            format!("Custom.{}x{}mm", custom.width_mm, custom.height_mm)
        }
    }
}

/// Rasterizes on a blocking thread so CPU-bound page rendering never stalls
/// the async runtime, while still honouring cancellation and the deadline.
/// Submits a validated PDF through `lp`, honouring cancellation and a deadline.
///
/// The temporary copy is created with owner-only permissions and removed when
/// this function returns, after `lp` has finished reading it.
#[cfg(unix)]
pub(crate) async fn submit_cups_pdf(
    queue_name: &str,
    document: &PdfPrintDocument,
    deadline: Duration,
    cancellation: &CancellationToken,
) -> SpoolerResult<SubmissionReceipt> {
    let file = tempfile::Builder::new()
        .prefix("oppa-document-")
        .suffix(".pdf")
        .tempfile()
        .map_err(|error| SpoolerError::BackendUnavailable(error.to_string()))?;
    std::fs::write(file.path(), &document.bytes)
        .map_err(|error| SpoolerError::Connectivity(error.to_string()))?;

    let mut command = tokio::process::Command::new("lp");
    command
        .args(cups_pdf_args(queue_name, &document.settings, file.path()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let operation = async {
        let output = command.output().await.map_err(|error| {
            SpoolerError::BackendUnavailable(format!("cannot start lp: {error}"))
        })?;
        lp_receipt(queue_name, output, "cups-pdf", "pdf", Some("pdf"), None)
    };
    let result = tokio::select! {
        () = cancellation.cancelled() => Err(SpoolerError::Cancelled),
        result = tokio::time::timeout(deadline, operation) => result.map_err(|_| SpoolerError::Timeout {
            stage: "system queue submission",
            duration: deadline,
        })?,
    };
    drop(file);
    result
}

/// Windows raster fallback: rasterizes the pages and submits them via GDI.
///
/// Duplex is rejected before this point because the GDI path uses the driver's
/// default device mode.
#[cfg(windows)]
pub(crate) fn windows_print_pdf(
    queue_name: &str,
    document: &PdfPrintDocument,
    deadline: Duration,
) -> SpoolerResult<SubmissionReceipt> {
    let raster = rasterize_for_driver(document, deadline)?;
    windows_print_driver(queue_name, &raster, None)
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::Path;

    use oppa_protocol::{
        CustomPaperSize, Duplex, Orientation, PaperSize, PrintSettings, Scaling, StandardPaperSize,
    };

    use super::*;

    fn args_as_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn default_settings_pin_actual_scale_and_one_sided_output() {
        let args = args_as_strings(&cups_pdf_args(
            "Office",
            &PrintSettings::default(),
            Path::new("/tmp/document.pdf"),
        ));
        assert_eq!(
            args,
            vec![
                "-d",
                "Office",
                "-n",
                "1",
                "-o",
                "sides=one-sided",
                "-o",
                "print-scaling=none",
                "/tmp/document.pdf",
            ]
        );
    }

    #[test]
    fn full_settings_map_to_ipp_attributes() {
        let settings = PrintSettings {
            copies: Some(3),
            paper: Some(PaperSize::Standard(StandardPaperSize::A5)),
            orientation: Some(Orientation::Landscape),
            scaling: Some(Scaling::Percent(80)),
            page_ranges: Some("1-2,5".to_owned()),
            duplex: Some(Duplex::LongEdge),
        };
        let args = args_as_strings(&cups_pdf_args("Office", &settings, Path::new("/tmp/d.pdf")));
        for expected in [
            "-n",
            "3",
            "-P",
            "1-2,5",
            "-o",
            "media=A5",
            "orientation-requested=4",
            "sides=two-sided-long-edge",
            "scaling=80",
        ] {
            assert!(
                args.iter().any(|value| value == expected),
                "missing {expected}: {args:?}"
            );
        }
    }

    #[test]
    fn custom_label_media_uses_cups_custom_name() {
        let settings = PrintSettings {
            paper: Some(PaperSize::Custom(CustomPaperSize {
                width_mm: 50,
                height_mm: 30,
            })),
            ..PrintSettings::default()
        };
        let args = args_as_strings(&cups_pdf_args("Label", &settings, Path::new("/tmp/d.pdf")));
        assert!(args.iter().any(|value| value == "media=Custom.50x30mm"));
    }
}
