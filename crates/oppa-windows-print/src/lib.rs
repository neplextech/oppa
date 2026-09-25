//! Safe orchestration for silent Windows GDI page printing.
//!
//! The platform-specific GDI calls live in [`gdi`]. This module keeps the
//! document and page lifecycle testable with a fake driver API.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use oppa_renderer::{RasterDocument, RasterPage};
use thiserror::Error;

#[cfg(windows)]
#[allow(unsafe_code)]
mod gdi;

/// Narrow sequence of operations needed to submit GDI page content.
pub trait DriverPrintApi {
    /// Platform print context, such as a GDI device context.
    type Context;

    /// Opens the named operating-system queue and creates its driver context.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the queue or context cannot be opened.
    fn open(&mut self, queue_name: &str) -> Result<Self::Context, String>;

    /// Starts one driver-managed print document and returns its queue job ID.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the driver rejects the document start.
    fn start_document(&mut self, context: &mut Self::Context) -> Result<u32, String>;

    /// Starts one page in the active document.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the driver cannot start the page.
    fn start_page(&mut self, context: &mut Self::Context) -> Result<(), String>;

    /// Draws one device-independent raster page through the driver context.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the page cannot be drawn.
    fn draw_page(&mut self, context: &mut Self::Context, page: &RasterPage) -> Result<(), String>;

    /// Ends the active page.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the driver cannot finish the page.
    fn end_page(&mut self, context: &mut Self::Context) -> Result<(), String>;

    /// Ends and commits the document to the Windows spooler.
    ///
    /// # Errors
    ///
    /// Returns the platform detail if the driver cannot commit the document.
    fn end_document(&mut self, context: &mut Self::Context) -> Result<(), String>;

    /// Cancels an active document after a partial failure.
    fn abort_document(&mut self, context: &mut Self::Context);

    /// Releases every resource owned by the context.
    fn close(&mut self, context: Self::Context);
}

/// Stages at which the driver print sequence can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintStage {
    /// Queue/device context creation.
    Open,
    /// Document start.
    StartDocument,
    /// Page start.
    StartPage,
    /// Raster drawing.
    DrawPage,
    /// Page completion.
    EndPage,
    /// Document submission.
    EndDocument,
}

/// Failure while submitting a raster page through an operating-system driver.
#[derive(Debug, Error)]
#[error("Windows driver printing failed during {stage:?}: {message}")]
pub struct PrintError {
    /// Operation that failed.
    pub stage: PrintStage,
    /// Sanitized driver failure detail.
    pub message: String,
}

/// Submits every page through a driver API and guarantees cleanup on failure.
///
/// # Errors
///
/// Returns a [`PrintError`] identifying the failed lifecycle stage. A started
/// document is aborted and its context is closed after any later-stage error.
pub fn print_with_api<A: DriverPrintApi>(
    api: &mut A,
    queue_name: &str,
    document: &RasterDocument,
) -> Result<u32, PrintError> {
    if document.pages.is_empty() {
        return Err(PrintError {
            stage: PrintStage::DrawPage,
            message: "document has no pages".to_owned(),
        });
    }
    for page in &document.pages {
        page.validate().map_err(|error| PrintError {
            stage: PrintStage::DrawPage,
            message: error.to_string(),
        })?;
    }

    let mut context = api.open(queue_name).map_err(|message| PrintError {
        stage: PrintStage::Open,
        message,
    })?;
    let job_id = match api.start_document(&mut context) {
        Ok(job_id) => job_id,
        Err(message) => {
            api.close(context);
            return Err(PrintError {
                stage: PrintStage::StartDocument,
                message,
            });
        }
    };
    let result = print_started_document(api, &mut context, document, job_id);
    if result.is_err() {
        api.abort_document(&mut context);
    }
    api.close(context);
    result
}

fn print_started_document<A: DriverPrintApi>(
    api: &mut A,
    context: &mut A::Context,
    document: &RasterDocument,
    job_id: u32,
) -> Result<u32, PrintError> {
    for page in &document.pages {
        api.start_page(context).map_err(|message| PrintError {
            stage: PrintStage::StartPage,
            message,
        })?;
        api.draw_page(context, page).map_err(|message| PrintError {
            stage: PrintStage::DrawPage,
            message,
        })?;
        api.end_page(context).map_err(|message| PrintError {
            stage: PrintStage::EndPage,
            message,
        })?;
    }
    api.end_document(context).map_err(|message| PrintError {
        stage: PrintStage::EndDocument,
        message,
    })?;
    Ok(job_id)
}

/// Prints pages through the Windows GDI printer driver without a dialog.
///
/// # Errors
///
/// Returns a [`PrintError`] if the queue, document, page, or driver drawing
/// operation fails.
#[cfg(windows)]
pub fn print_document(queue_name: &str, document: &RasterDocument) -> Result<u32, PrintError> {
    print_with_api(&mut gdi::GdiPrintApi, queue_name, document)
}

#[cfg(test)]
mod tests {
    use oppa_renderer::{PixelFormat, RasterDocument, RasterPage};

    use super::{DriverPrintApi, PrintStage, print_with_api};

    #[derive(Default)]
    struct FakeApi {
        events: Vec<&'static str>,
        fail_at: Option<&'static str>,
    }

    impl FakeApi {
        fn event(&mut self, event: &'static str) -> Result<(), String> {
            self.events.push(event);
            if self.fail_at == Some(event) {
                Err(format!("failure at {event}"))
            } else {
                Ok(())
            }
        }
    }

    impl DriverPrintApi for FakeApi {
        type Context = ();

        fn open(&mut self, _queue_name: &str) -> Result<Self::Context, String> {
            self.event("open")?;
            Ok(())
        }

        fn start_document(&mut self, _context: &mut Self::Context) -> Result<u32, String> {
            self.event("start-document")?;
            Ok(27)
        }

        fn start_page(&mut self, _context: &mut Self::Context) -> Result<(), String> {
            self.event("start-page")
        }

        fn draw_page(
            &mut self,
            _context: &mut Self::Context,
            _page: &RasterPage,
        ) -> Result<(), String> {
            self.event("draw-page")
        }

        fn end_page(&mut self, _context: &mut Self::Context) -> Result<(), String> {
            self.event("end-page")
        }

        fn end_document(&mut self, _context: &mut Self::Context) -> Result<(), String> {
            self.event("end-document")
        }

        fn abort_document(&mut self, _context: &mut Self::Context) {
            self.events.push("abort-document");
        }

        fn close(&mut self, _context: Self::Context) {
            self.events.push("close");
        }
    }

    fn document() -> RasterDocument {
        RasterDocument {
            pages: vec![RasterPage {
                width: 8,
                height: 2,
                dpi_x: 300,
                dpi_y: 300,
                format: PixelFormat::Mono1,
                data: vec![0; 2],
            }],
        }
    }

    #[test]
    fn driver_sequence_starts_draws_and_commits_before_cleanup() {
        let mut api = FakeApi::default();
        let job_id = print_with_api(&mut api, "Office", &document()).expect("driver print");
        assert_eq!(job_id, 27);
        assert_eq!(
            api.events,
            [
                "open",
                "start-document",
                "start-page",
                "draw-page",
                "end-page",
                "end-document",
                "close"
            ]
        );
    }

    #[test]
    fn partial_failures_abort_and_close_the_context() {
        let failures = [
            ("open", PrintStage::Open, vec!["open"]),
            (
                "start-document",
                PrintStage::StartDocument,
                vec!["open", "start-document", "close"],
            ),
            (
                "start-page",
                PrintStage::StartPage,
                vec![
                    "open",
                    "start-document",
                    "start-page",
                    "abort-document",
                    "close",
                ],
            ),
            (
                "draw-page",
                PrintStage::DrawPage,
                vec![
                    "open",
                    "start-document",
                    "start-page",
                    "draw-page",
                    "abort-document",
                    "close",
                ],
            ),
            (
                "end-page",
                PrintStage::EndPage,
                vec![
                    "open",
                    "start-document",
                    "start-page",
                    "draw-page",
                    "end-page",
                    "abort-document",
                    "close",
                ],
            ),
            (
                "end-document",
                PrintStage::EndDocument,
                vec![
                    "open",
                    "start-document",
                    "start-page",
                    "draw-page",
                    "end-page",
                    "end-document",
                    "abort-document",
                    "close",
                ],
            ),
        ];
        for (failure, stage, events) in failures {
            let mut api = FakeApi {
                events: Vec::new(),
                fail_at: Some(failure),
            };
            let error = print_with_api(&mut api, "Office", &document())
                .expect_err("injected driver failure must propagate");
            assert_eq!(error.stage, stage);
            assert_eq!(api.events, events, "cleanup sequence after {failure}");
        }
    }

    #[test]
    fn invalid_raster_is_rejected_before_opening_the_queue() {
        let mut api = FakeApi::default();
        let mut invalid = document();
        invalid.pages[0].data.clear();
        let error = print_with_api(&mut api, "Office", &invalid).expect_err("invalid page");
        assert_eq!(error.stage, PrintStage::DrawPage);
        assert!(api.events.is_empty());
    }
}
