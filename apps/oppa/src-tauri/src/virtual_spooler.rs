use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use oppa_core::PrinterId;
use oppa_printer::{ConnectionKind, SubmissionReceipt};
use oppa_spooler::{
    Spooler, SpoolerError, SpoolerResult, SubmissionRequest, VirtualSimulation, VirtualSpooler,
    VirtualSubmission,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::models::VirtualPrinterMode;

pub const PRINTER_SOUND_EVENT: &str = "oppa://printer-sound";
const SOUND_MIN_DELAY_MS: u64 = 3_500;

struct VirtualDevice {
    spooler: VirtualSpooler,
    policy: Mutex<(VirtualPrinterMode, u64)>,
    submission: Mutex<()>,
    sound_enabled: AtomicBool,
}

impl VirtualDevice {
    fn new(mode: VirtualPrinterMode, delay_ms: u64) -> Self {
        Self {
            spooler: VirtualSpooler::default(),
            policy: Mutex::new((mode, delay_ms)),
            submission: Mutex::new(()),
            sound_enabled: AtomicBool::new(false),
        }
    }
}

/// Routes each virtual printer through an isolated bounded `VirtualSpooler`.
#[derive(Default)]
pub struct PerPrinterVirtualSpooler {
    app: Option<AppHandle>,
    devices: RwLock<HashMap<PrinterId, Arc<VirtualDevice>>>,
}

impl PerPrinterVirtualSpooler {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app: Some(app),
            devices: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, id: PrinterId, mode: VirtualPrinterMode, delay_ms: u64) {
        let device = self
            .devices
            .write()
            .await
            .entry(id)
            .or_insert_with(|| Arc::new(VirtualDevice::new(mode, delay_ms)))
            .clone();
        *device.policy.lock().await = (mode, delay_ms);
    }

    pub async fn set_sound(&self, id: &PrinterId, enabled: bool) -> Result<(), SpoolerError> {
        let device = self.device(id).await?;
        device.sound_enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    pub async fn remove(&self, id: &PrinterId) {
        self.devices.write().await.remove(id);
    }

    pub async fn policy(&self, id: &PrinterId) -> Result<(VirtualPrinterMode, u64), SpoolerError> {
        let device = self.device(id).await?;
        let policy = *device.policy.lock().await;
        Ok(policy)
    }

    pub async fn history(&self, id: &PrinterId) -> Result<Vec<VirtualSubmission>, SpoolerError> {
        Ok(self.device(id).await?.spooler.history().await)
    }

    pub async fn clear_history(&self, id: &PrinterId) -> Result<(), SpoolerError> {
        self.device(id).await?.spooler.clear_history().await;
        Ok(())
    }

    async fn device(&self, id: &PrinterId) -> Result<Arc<VirtualDevice>, SpoolerError> {
        self.devices.read().await.get(id).cloned().ok_or_else(|| {
            SpoolerError::BackendUnavailable("virtual printer is not registered".to_owned())
        })
    }
}

#[async_trait]
impl Spooler for PerPrinterVirtualSpooler {
    fn connection_kind(&self) -> ConnectionKind {
        ConnectionKind::Virtual
    }

    async fn submit(
        &self,
        request: SubmissionRequest<'_>,
        cancellation: &CancellationToken,
    ) -> SpoolerResult<SubmissionReceipt> {
        let device = self.device(&request.printer.id).await?;
        let _submission = device.submission.lock().await;
        let simulation = {
            let mut policy = device.policy.lock().await;
            match policy.0 {
                VirtualPrinterMode::AlwaysSucceed => VirtualSimulation::AlwaysSucceed,
                VirtualPrinterMode::FailNext => {
                    policy.0 = VirtualPrinterMode::AlwaysSucceed;
                    VirtualSimulation::FailNext
                }
                VirtualPrinterMode::AlwaysFail => VirtualSimulation::AlwaysFail,
                VirtualPrinterMode::Delay => {
                    VirtualSimulation::Delay(Duration::from_millis(policy.1))
                }
                VirtualPrinterMode::Offline => VirtualSimulation::Offline,
            }
        };
        device.spooler.set_simulation(simulation).await;

        if device.sound_enabled.load(Ordering::Relaxed) {
            if let Some(app) = &self.app {
                let _ = app.emit(PRINTER_SOUND_EVENT, request.printer.id.to_string());
            }
            let ((), result) = tokio::join!(
                tokio::time::sleep(Duration::from_millis(SOUND_MIN_DELAY_MS)),
                device.spooler.submit(request, cancellation)
            );
            result
        } else {
            device.spooler.submit(request, cancellation).await
        }
    }
}

#[cfg(test)]
mod tests {
    use oppa_core::{PrintJobId, PrinterId};
    use oppa_printer::{PrinterConnection, PrinterRef};
    use oppa_renderer::{RenderedDocument, VirtualPrintDocument};
    use oppa_spooler::{Spooler, SubmissionRequest};
    use tokio_util::sync::CancellationToken;

    use super::PerPrinterVirtualSpooler;
    use crate::models::VirtualPrinterMode;

    #[tokio::test]
    async fn fail_next_is_scoped_and_resets_for_one_printer() {
        let router = PerPrinterVirtualSpooler::default();
        let first = PrinterId::new("virtual-one").expect("id");
        let second = PrinterId::new("virtual-two").expect("id");
        router
            .register(first.clone(), VirtualPrinterMode::FailNext, 0)
            .await;
        router
            .register(second.clone(), VirtualPrinterMode::AlwaysSucceed, 0)
            .await;

        let first_ref = printer(first);
        let second_ref = printer(second);
        let document = RenderedDocument::Virtual(VirtualPrintDocument {
            document: serde_json::from_value(serde_json::json!({
                "width": 58,
                "sections": [{ "type": "text", "value": "test" }]
            }))
            .expect("document"),
            preview_lines: vec!["test".to_owned()],
        });
        let job = PrintJobId::new("job-test").expect("job");
        let cancellation = CancellationToken::new();

        assert!(
            router
                .submit(
                    SubmissionRequest {
                        job_id: &job,
                        printer: &first_ref,
                        document: &document,
                    },
                    &cancellation,
                )
                .await
                .is_err()
        );
        assert_eq!(
            router.policy(&first_ref.id).await.expect("policy").0,
            VirtualPrinterMode::AlwaysSucceed
        );
        assert!(
            router
                .submit(
                    SubmissionRequest {
                        job_id: &job,
                        printer: &second_ref,
                        document: &document,
                    },
                    &cancellation,
                )
                .await
                .is_ok()
        );
    }

    fn printer(id: PrinterId) -> PrinterRef {
        PrinterRef {
            connection: PrinterConnection::Virtual {
                printer_id: id.to_string(),
            },
            id,
            display_name: "Virtual".to_owned(),
            enabled: true,
        }
    }
}
