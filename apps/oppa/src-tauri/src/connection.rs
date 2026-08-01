use std::{
    collections::VecDeque,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use async_trait::async_trait;
use oppa_agent::{
    AgentRuntimeError, AgentState, OutboundReportError, OutboundReporter, ReceiveJobOutcome,
    ServerJobOutcome,
};
use oppa_core::Timestamp;
use oppa_protocol::{
    AgentHeartbeat, AgentHello, AgentMessage, AgentMessageKind, GatewayAuthenticationResponse,
    GatewayAuthenticationServerMessage, PrinterInventory, ProtocolVersion, ServerMessage,
    ServerMessageKind,
};
use oppa_transport::{
    AgentTransport, BackoffPolicy, ReconnectBackoff, TransportConfig, TransportError,
    WebSocketTransport,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    error::sanitize,
    models::{ConnectedServiceSummary, DesktopJobState, JobSummary, OpenPrinterConnectionState},
    service::{DesktopService, JOBS_CHANGED_EVENT, PRINTERS_CHANGED_EVENT},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const SERVER_HELLO_TIMEOUT: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const OUTBOUND_BUFFER_CAPACITY: usize = 256;

/// Host request that needs confirmation from the connection actor.
pub struct OutboundRequest {
    message: AgentMessage,
    completion: oneshot::Sender<Result<(), String>>,
}

/// Bounded channel reporter used by `oppa-agent` without depending on Tauri.
pub struct OutboundChannelReporter {
    sender: mpsc::Sender<OutboundRequest>,
}

impl OutboundChannelReporter {
    pub fn new(sender: mpsc::Sender<OutboundRequest>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl OutboundReporter for OutboundChannelReporter {
    async fn report(&self, message: &AgentMessage) -> Result<(), OutboundReportError> {
        let (completion, result) = oneshot::channel();
        self.sender
            .send(OutboundRequest {
                message: message.clone(),
                completion,
            })
            .await
            .map_err(|_| OutboundReportError::new("connection reporter is unavailable"))?;
        result
            .await
            .map_err(|_| OutboundReportError::new("connection reporter stopped"))?
            .map_err(OutboundReportError::new)
    }
}

/// Commands sent from tray/frontend operations to the connection actor.
#[derive(Debug, Clone, Copy)]
pub enum ConnectionControl {
    Reconnect,
    ConfigurationChanged,
    PublishInventory,
    Shutdown,
}

/// Owns the mutable WebSocket and reconnect lifecycle for the desktop host.
#[allow(clippy::too_many_lines)]
pub async fn run_connection_supervisor(
    service: Arc<DesktopService>,
    mut outbound: mpsc::Receiver<OutboundRequest>,
    mut controls: mpsc::Receiver<ConnectionControl>,
) {
    let mut backoff = match ReconnectBackoff::new(BackoffPolicy::default()) {
        Ok(backoff) => backoff,
        Err(error) => {
            service.set_connection_error(error.to_string()).await;
            return;
        }
    };
    let mut automatic_reconnect = true;
    let mut retry_after = None;

    loop {
        if service.shutdown.is_cancelled() {
            finish_outbound(&mut outbound, "application is shutting down");
            return;
        }
        if !automatic_reconnect {
            match wait_for_reconnect(&service, &mut controls, &mut outbound).await {
                Some(ConnectionControl::Reconnect) => {
                    automatic_reconnect = true;
                    backoff.reset();
                }
                Some(
                    ConnectionControl::ConfigurationChanged | ConnectionControl::PublishInventory,
                ) => continue,
                Some(ConnectionControl::Shutdown) | None => {
                    finish_outbound(&mut outbound, "application is shutting down");
                    return;
                }
            }
        }
        if let Some(delay) = retry_after.take() {
            match wait_delay_or_control(&service, delay, &mut controls).await {
                DelayOutcome::Continue => {}
                DelayOutcome::Reconnect => backoff.reset(),
                DelayOutcome::Pause => {
                    automatic_reconnect = false;
                    continue;
                }
                DelayOutcome::Shutdown => {
                    finish_outbound(&mut outbound, "application is shutting down");
                    return;
                }
            }
        }

        let (connection, server_url, provider_generation) = {
            let _provider_operation = service.provider_operation.lock().await;
            let server_configuration = service.server_configuration.read().await.clone();
            (
                service.connection.read().await.clone(),
                server_configuration.server_url,
                service.provider_generation.load(Ordering::Acquire),
            )
        };
        let Some(connection) = connection else {
            automatic_reconnect = false;
            continue;
        };
        let discovered = match service.pairing_client.discover(&server_url).await {
            Ok(discovered) => discovered,
            Err(error) => {
                service
                    .set_connection_phase(OpenPrinterConnectionState::DiscoveryFailed)
                    .await;
                service.set_connection_error(error.to_string()).await;
                retry_after = next_retry_delay(&mut backoff);
                if retry_after.is_none() {
                    automatic_reconnect = false;
                }
                continue;
            }
        };
        if discovered.document.server.id != connection.server_id {
            service
                .authentication_failed(
                    "The configured URL now identifies a different server. Forget it and pair again.",
                    false,
                )
                .await;
            automatic_reconnect = false;
            continue;
        }
        let gateway_endpoint = discovered.gateway_url.clone();
        {
            let _provider_operation = service.provider_operation.lock().await;
            if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                automatic_reconnect = false;
                continue;
            }
            *service.agent_id.write().await = Some(connection.agent_id.clone());
            *service.connected_service.write().await = None;
            service
                .set_connection_phase(OpenPrinterConnectionState::Connecting)
                .await;
            service.transition(AgentState::Connecting).await;
        }

        let config = TransportConfig {
            endpoint: gateway_endpoint.clone(),
            connect_timeout: CONNECT_TIMEOUT,
            idle_timeout: IDLE_TIMEOUT,
            outbound_buffer_capacity: OUTBOUND_BUFFER_CAPACITY,
        };
        let mut transport = match WebSocketTransport::new(config) {
            Ok(transport) => transport,
            Err(error) => {
                let _provider_operation = service.provider_operation.lock().await;
                if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                    automatic_reconnect = false;
                    continue;
                }
                service.transition(AgentState::Disconnected).await;
                service.set_connection_error(error.to_string()).await;
                automatic_reconnect = false;
                continue;
            }
        };
        if let Err(error) = transport.connect().await {
            {
                let _provider_operation = service.provider_operation.lock().await;
                if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                    automatic_reconnect = false;
                    continue;
                }
                service.transition(AgentState::Disconnected).await;
                service.set_connection_error(error.to_string()).await;
            }
            if !error.is_retryable() {
                automatic_reconnect = false;
                continue;
            }
            let Some((_attempt, delay)) = backoff.next_delay().ok().flatten() else {
                automatic_reconnect = false;
                continue;
            };
            match wait_delay_or_control(&service, delay, &mut controls).await {
                DelayOutcome::Continue => {}
                DelayOutcome::Reconnect => backoff.reset(),
                DelayOutcome::Pause => {
                    automatic_reconnect = false;
                    continue;
                }
                DelayOutcome::Shutdown => {
                    finish_outbound(&mut outbound, "application is shutting down");
                    return;
                }
            }
            continue;
        }
        {
            let _provider_operation = service.provider_operation.lock().await;
            if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                let _ = transport.close().await;
                automatic_reconnect = false;
                continue;
            }
        }

        service
            .set_connection_phase(OpenPrinterConnectionState::Authenticating)
            .await;
        let challenge = match transport.receive_authentication().await {
            Ok(GatewayAuthenticationServerMessage::Challenge(challenge)) => challenge,
            Ok(_) => {
                service
                    .authentication_failed(
                        "Gateway did not send an authentication challenge.",
                        false,
                    )
                    .await;
                automatic_reconnect = false;
                continue;
            }
            Err(error) => {
                service
                    .authentication_failed(error.to_string(), false)
                    .await;
                automatic_reconnect = false;
                continue;
            }
        };
        let signature = match service
            .key_manager
            .sign_challenge(&connection.credential_ref, &challenge)
            .await
        {
            Ok(signature) => signature,
            Err(error) => {
                service
                    .authentication_failed(error.to_string(), false)
                    .await;
                automatic_reconnect = false;
                continue;
            }
        };
        if let Err(error) = transport
            .send_authentication_response(GatewayAuthenticationResponse {
                message_type: "auth.response".to_owned(),
                challenge_id: challenge.challenge_id,
                agent_id: connection.agent_id.clone(),
                key_id: connection.key_id.clone(),
                algorithm: oppa_protocol::SIGNATURE_ALGORITHM.to_owned(),
                signature,
            })
            .await
        {
            service.set_connection_error(error.to_string()).await;
            retry_after = next_retry_delay(&mut backoff);
            continue;
        }
        match transport.receive_authentication().await {
            Ok(GatewayAuthenticationServerMessage::Accepted(accepted))
                if accepted.agent_id == connection.agent_id => {}
            Ok(_) => {
                service
                    .authentication_failed("Gateway authentication response was invalid.", false)
                    .await;
                automatic_reconnect = false;
                continue;
            }
            Err(TransportError::AuthenticationRejected { code }) => {
                let revoked = code == "credential_revoked";
                service
                    .authentication_failed(
                        format!("Gateway authentication failed: {code}"),
                        revoked,
                    )
                    .await;
                automatic_reconnect = false;
                continue;
            }
            Err(error) => {
                service.set_connection_error(error.to_string()).await;
                retry_after = next_retry_delay(&mut backoff);
                continue;
            }
        }

        let hello = hello_message(&service, &connection.agent_id);
        let hello_message_id = hello.message_id.clone();
        if let Err(error) = transport.send(hello).await {
            let _provider_operation = service.provider_operation.lock().await;
            if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                automatic_reconnect = false;
                continue;
            }
            service.transition(AgentState::Disconnected).await;
            service.set_connection_error(error.to_string()).await;
            retry_after = next_retry_delay(&mut backoff);
            if retry_after.is_none() {
                automatic_reconnect = false;
            }
            continue;
        }
        service.log.info(
            "transport",
            "Gateway socket established; waiting for server hello.",
        );

        let mut handshaken = false;
        let mut awaiting_handshake = VecDeque::new();
        let mut connection_ended = false;
        let hello_timeout = tokio::time::sleep(SERVER_HELLO_TIMEOUT);
        tokio::pin!(hello_timeout);
        while !connection_ended {
            tokio::select! {
                () = &mut hello_timeout, if !handshaken => {
                    let _provider_operation = service.provider_operation.lock().await;
                    if service.provider_generation.load(Ordering::Acquire) == provider_generation {
                        service
                            .set_connection_error("Gateway did not complete the server hello handshake in time.")
                            .await;
                        reject_pending(&mut awaiting_handshake, "gateway hello timed out");
                        retry_after = next_retry_delay(&mut backoff);
                        if retry_after.is_none() {
                            automatic_reconnect = false;
                        }
                    } else {
                        automatic_reconnect = false;
                        reject_pending(&mut awaiting_handshake, "server configuration changed");
                    }
                    connection_ended = true;
                }
                () = service.shutdown.cancelled() => {
                    let _ = transport.close().await;
                    reject_pending(&mut awaiting_handshake, "application is shutting down");
                    finish_outbound(&mut outbound, "application is shutting down");
                    return;
                }
                control = controls.recv() => {
                    match control {
                        Some(ConnectionControl::Reconnect) => {
                            let _ = transport.close().await;
                            reject_pending(&mut awaiting_handshake, "connection was restarted");
                            backoff.reset();
                            connection_ended = true;
                        }
                        Some(ConnectionControl::ConfigurationChanged) => {
                            let _ = transport.close().await;
                            reject_pending(&mut awaiting_handshake, "server configuration changed");
                            automatic_reconnect = false;
                            connection_ended = true;
                        }
                        Some(ConnectionControl::PublishInventory) if handshaken => {
                            let _provider_operation = service.provider_operation.lock().await;
                            if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                                automatic_reconnect = false;
                                connection_ended = true;
                            } else if let Err(error) = send_inventory(&service, &mut transport, None).await {
                                service.set_connection_error(error.to_string()).await;
                                retry_after = next_retry_delay(&mut backoff);
                                if retry_after.is_none() {
                                    automatic_reconnect = false;
                                }
                                connection_ended = true;
                            }
                        }
                        Some(ConnectionControl::PublishInventory) => {}
                        Some(ConnectionControl::Shutdown) | None => {
                            let _ = transport.close().await;
                            reject_pending(&mut awaiting_handshake, "application is shutting down");
                            finish_outbound(&mut outbound, "application is shutting down");
                            return;
                        }
                    }
                }
                request = outbound.recv() => {
                    let Some(request) = request else {
                        let _ = transport.close().await;
                        return;
                    };
                    let _provider_operation = service.provider_operation.lock().await;
                    if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                        let _ = request.completion.send(Err("server configuration changed".to_owned()));
                        automatic_reconnect = false;
                        connection_ended = true;
                    } else if handshaken {
                        match transport.send(request.message).await {
                            Ok(()) => {
                                let _ = request.completion.send(Ok(()));
                            }
                            Err(error) => {
                                let diagnostic = sanitize(&error.to_string());
                                let _ = request.completion.send(Err(diagnostic.clone()));
                                service.set_connection_error(diagnostic).await;
                                retry_after = next_retry_delay(&mut backoff);
                                if retry_after.is_none() {
                                    automatic_reconnect = false;
                                }
                                connection_ended = true;
                            }
                        }
                    } else if awaiting_handshake.len() < OUTBOUND_BUFFER_CAPACITY {
                        awaiting_handshake.push_back(request);
                    } else {
                        let _ = request.completion.send(Err(
                            "outbound reports exceeded the pre-handshake bound".to_owned()
                        ));
                    }
                }
                incoming = transport.receive() => {
                    match incoming {
                        Ok(message) => {
                            let was_handshaken = handshaken;
                            match handle_server_message(
                                &service,
                                &mut transport,
                                message,
                                &hello_message_id,
                                &mut handshaken,
                                &mut awaiting_handshake,
                                gateway_endpoint.as_str(),
                                provider_generation,
                            )
                            .await
                            {
                                MessageDisposition::Continue => {}
                                MessageDisposition::Reconnect { delay } => {
                                    retry_after =
                                        delay.or_else(|| next_retry_delay(&mut backoff));
                                    if retry_after.is_none() {
                                        automatic_reconnect = false;
                                    }
                                    connection_ended = true;
                                }
                                MessageDisposition::Stop => {
                                    automatic_reconnect = false;
                                    connection_ended = true;
                                }
                            }
                            if !was_handshaken && handshaken {
                                backoff.reset();
                            }
                        }
                        Err(error) => {
                            let _provider_operation = service.provider_operation.lock().await;
                            if service.provider_generation.load(Ordering::Acquire) != provider_generation {
                                reject_pending(&mut awaiting_handshake, "server configuration changed");
                                automatic_reconnect = false;
                                connection_ended = true;
                                continue;
                            }
                            let retryable = error.is_retryable();
                            service.set_connection_error(error.to_string()).await;
                            reject_pending(&mut awaiting_handshake, &error.to_string());
                            if retryable {
                                retry_after = backoff
                                    .next_delay()
                                    .ok()
                                    .flatten()
                                    .map(|(_attempt, delay)| delay);
                            } else {
                                automatic_reconnect = false;
                            }
                            connection_ended = true;
                        }
                    }
                }
            }
        }
        let _ = transport.close().await;
        let _provider_operation = service.provider_operation.lock().await;
        if service.provider_generation.load(Ordering::Acquire) == provider_generation {
            *service.connected_service.write().await = None;
            let phase = *service.connection_state.read().await;
            if matches!(
                phase,
                OpenPrinterConnectionState::Connecting
                    | OpenPrinterConnectionState::Authenticating
                    | OpenPrinterConnectionState::Connected
            ) {
                service
                    .set_connection_phase(OpenPrinterConnectionState::Paired)
                    .await;
            }
            service.transition(AgentState::Disconnected).await;
        }
    }
}

enum MessageDisposition {
    Continue,
    Reconnect { delay: Option<Duration> },
    Stop,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn handle_server_message(
    service: &Arc<DesktopService>,
    transport: &mut WebSocketTransport,
    message: ServerMessage,
    hello_message_id: &str,
    handshaken: &mut bool,
    awaiting_handshake: &mut VecDeque<OutboundRequest>,
    gateway_url: &str,
    provider_generation: u64,
) -> MessageDisposition {
    let _provider_operation = service.provider_operation.lock().await;
    if service.provider_generation.load(Ordering::Acquire) != provider_generation {
        return MessageDisposition::Stop;
    }
    if !*handshaken && !matches!(&message.kind, ServerMessageKind::Hello(_)) {
        service
            .set_connection_error(format!(
                "Gateway sent {} before completing the server hello handshake.",
                message.message_type()
            ))
            .await;
        return MessageDisposition::Reconnect { delay: None };
    }
    match &message.kind {
        ServerMessageKind::Hello(hello) => {
            if *handshaken || message.correlation_id.as_deref() != Some(hello_message_id) {
                service
                    .set_connection_error("Gateway hello did not match the active handshake.")
                    .await;
                return MessageDisposition::Reconnect { delay: None };
            }
            *handshaken = true;
            *service.last_connection_at.write().await = Some(Timestamp::now().to_string());
            *service.connected_service.write().await = Some(ConnectedServiceSummary {
                name: hello.brand.name.clone(),
                server_id: hello.server_id.clone(),
                server_version: hello.server_version.clone(),
                gateway_url: gateway_url.to_owned(),
            });
            service.agent.handle().set_active_errors(Vec::new()).await;
            service
                .set_connection_phase(OpenPrinterConnectionState::Connected)
                .await;
            service.transition(AgentState::Connected).await;
            service.log.info(
                "transport",
                format!(
                    "Authenticated connection to {} established.",
                    hello.brand.name
                ),
            );

            if let Err(error) = send_inventory(service, transport, None).await {
                service.set_connection_error(error.to_string()).await;
                return MessageDisposition::Reconnect { delay: None };
            }
            while let Some(request) = awaiting_handshake.pop_front() {
                match transport.send(request.message).await {
                    Ok(()) => {
                        let _ = request.completion.send(Ok(()));
                    }
                    Err(error) => {
                        let diagnostic = sanitize(&error.to_string());
                        let _ = request.completion.send(Err(diagnostic.clone()));
                        reject_pending(awaiting_handshake, &diagnostic);
                        service.set_connection_error(diagnostic).await;
                        return MessageDisposition::Reconnect { delay: None };
                    }
                }
            }
            let recovery_service = Arc::clone(service);
            tauri::async_runtime::spawn(async move {
                let replayed_reports = match recovery_service.agent.replay_outbound_reports().await
                {
                    Ok(replayed) => replayed,
                    Err(error) => {
                        recovery_service.log.warn("job", error.to_string());
                        recovery_service.emit(JOBS_CHANGED_EVENT);
                        return;
                    }
                };
                match recovery_service.agent.recover().await {
                    Ok(recovery) => {
                        for outcome in &recovery.outcomes {
                            if let Err(error) =
                                recovery_service.apply_process_outcome(outcome).await
                            {
                                recovery_service.log.warn("job", error.to_string());
                            }
                        }
                        recovery_service.log.info(
                            "job",
                            format!(
                                "Startup recovery restored {} interrupted submission(s), replayed {replayed_reports} durable status report(s), and processed {} pending job(s).",
                                recovery_service.startup_recovered_submissions,
                                recovery.outcomes.len()
                            ),
                        );
                    }
                    Err(error) => {
                        recovery_service.log.warn("job", error.to_string());
                    }
                }
                recovery_service.emit(JOBS_CHANGED_EVENT);
            });
            MessageDisposition::Continue
        }
        ServerMessageKind::Heartbeat(_) => {
            if !*handshaken {
                return MessageDisposition::Reconnect { delay: None };
            }
            let heartbeat = envelope(
                AgentMessageKind::Heartbeat(AgentHeartbeat {
                    uptime_seconds: service.uptime_seconds(),
                }),
                Some(message.message_id.clone()),
            );
            if let Err(error) = transport.send(heartbeat).await {
                service.set_connection_error(error.to_string()).await;
                MessageDisposition::Reconnect { delay: None }
            } else {
                MessageDisposition::Continue
            }
        }
        ServerMessageKind::RequestPrinterInventory(_) => {
            if let Err(error) =
                send_inventory(service, transport, Some(message.message_id.clone())).await
            {
                service.set_connection_error(error.to_string()).await;
                MessageDisposition::Reconnect { delay: None }
            } else {
                MessageDisposition::Continue
            }
        }
        ServerMessageKind::ConfigurationInvalidated(_) => {
            if let Err(error) = service.refresh_printers().await {
                service.log.warn("discovery", error.to_string());
            }
            if let Err(error) = send_inventory(service, transport, None).await {
                service.set_connection_error(error.to_string()).await;
                MessageDisposition::Reconnect { delay: None }
            } else {
                MessageDisposition::Continue
            }
        }
        ServerMessageKind::PrintJob(job) => {
            let printer_name = service.catalog.get(&job.printer_id).await.map_or_else(
                |_| "Printer".to_owned(),
                |printer| printer.reference.display_name,
            );
            let now = Timestamp::now().to_string();
            if let Err(error) = service
                .jobs
                .insert(JobSummary {
                    id: job.job_id.clone(),
                    printer_id: job.printer_id.clone(),
                    printer_name,
                    idempotency_key: job.idempotency_key.clone(),
                    state: DesktopJobState::Queued,
                    received_at: now.clone(),
                    updated_at: now,
                    attempts: 0,
                    error: None,
                })
                .await
            {
                service.log.warn("job", error.to_string());
            }
            service.emit(JOBS_CHANGED_EVENT);
            let agent = Arc::clone(&service.agent);
            let job_service = Arc::clone(service);
            let message = message.clone();
            let job_id = job.job_id.clone();
            tauri::async_runtime::spawn(async move {
                match agent.handle_server_message(&message).await {
                    Ok(ServerJobOutcome::PrintJob(flow)) => match flow.receipt {
                        ReceiveJobOutcome::Inserted { .. } => {}
                        ReceiveJobOutcome::DuplicateJob { state, .. } => {
                            let state = desktop_job_state(state);
                            if let Err(error) = job_service
                                .jobs
                                .update(&job_id, state, None, None, Timestamp::now().to_string())
                                .await
                            {
                                job_service.log.warn("job", error.to_string());
                            }
                        }
                        ReceiveJobOutcome::DuplicateIdempotency {
                            existing_job_id, ..
                        } => {
                            if let Err(error) = job_service
                                .jobs
                                .fail_if_non_terminal(
                                    &job_id,
                                    format!(
                                        "Idempotency key already belongs to job {existing_job_id}."
                                    ),
                                )
                                .await
                            {
                                job_service.log.warn("job", error.to_string());
                            }
                        }
                    },
                    Ok(ServerJobOutcome::CancelJob(_)) => {}
                    Err(AgentRuntimeError::Reporting { message, source }) => {
                        let (state, error) = match message.kind {
                            AgentMessageKind::JobSubmitted(_) => (DesktopJobState::Submitted, None),
                            AgentMessageKind::JobFailed(failed) => {
                                (DesktopJobState::Failed, Some(failed.error.message))
                            }
                            AgentMessageKind::JobReceived(_) => (
                                DesktopJobState::Received,
                                Some("Receipt status is waiting for gateway delivery.".to_owned()),
                            ),
                            _ => (
                                DesktopJobState::Failed,
                                Some("Gateway status reporting failed.".to_owned()),
                            ),
                        };
                        if let Err(error) = job_service
                            .jobs
                            .update(&job_id, state, error, None, Timestamp::now().to_string())
                            .await
                        {
                            job_service.log.warn("job", error.to_string());
                        }
                        job_service.log.warn("transport", source.to_string());
                    }
                    Err(error) => {
                        if let Err(ledger_error) = job_service
                            .jobs
                            .fail_if_non_terminal(&job_id, error.to_string())
                            .await
                        {
                            job_service.log.warn("job", ledger_error.to_string());
                        }
                        job_service.log.warn("job", error.to_string());
                    }
                }
                job_service.emit(JOBS_CHANGED_EVENT);
                job_service.emit(PRINTERS_CHANGED_EVENT);
            });
            MessageDisposition::Continue
        }
        ServerMessageKind::CancelJob(_) => {
            let agent = Arc::clone(&service.agent);
            let cancel_service = Arc::clone(service);
            let message = message.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = agent.handle_server_message(&message).await {
                    cancel_service.log.warn("job", error.to_string());
                }
                cancel_service.emit(JOBS_CHANGED_EVENT);
            });
            MessageDisposition::Continue
        }
        ServerMessageKind::Disconnect(disconnect) => {
            service.log.warn(
                "transport",
                format!(
                    "Gateway requested disconnect ({}): {}",
                    disconnect.code,
                    sanitize(&disconnect.reason)
                ),
            );
            if disconnect.reconnect {
                MessageDisposition::Reconnect {
                    delay: disconnect
                        .retry_after_ms
                        .map(|value| Duration::from_millis(u64::from(value))),
                }
            } else {
                MessageDisposition::Stop
            }
        }
    }
}

fn desktop_job_state(state: oppa_core::JobState) -> DesktopJobState {
    match state {
        oppa_core::JobState::Received | oppa_core::JobState::Submitting => {
            DesktopJobState::Received
        }
        oppa_core::JobState::Submitted => DesktopJobState::Submitted,
        oppa_core::JobState::Failed => DesktopJobState::Failed,
        oppa_core::JobState::Cancelled => DesktopJobState::Cancelled,
    }
}

fn hello_message(service: &DesktopService, agent_id: &str) -> AgentMessage {
    envelope(
        AgentMessageKind::Hello(AgentHello {
            agent_id: agent_id.to_owned(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            product_id: service.product.product_id.to_string(),
            product_version: env!("CARGO_PKG_VERSION").to_owned(),
            supported_protocol_versions: vec![ProtocolVersion::CURRENT],
        }),
        None,
    )
}

async fn send_inventory(
    service: &DesktopService,
    transport: &mut WebSocketTransport,
    correlation_id: Option<String>,
) -> Result<(), TransportError> {
    let printers = service
        .catalog
        .protocol_descriptors()
        .await
        .map_err(|error| TransportError::ProtocolEncoding(error.to_string()))?;
    transport
        .send(envelope(
            AgentMessageKind::PrinterInventory(PrinterInventory {
                revision: service.catalog.revision(),
                printers,
            }),
            correlation_id,
        ))
        .await
}

fn envelope(kind: AgentMessageKind, correlation_id: Option<String>) -> AgentMessage {
    AgentMessage {
        protocol_version: ProtocolVersion::CURRENT,
        message_id: format!("message_{}", Uuid::new_v4()),
        sent_at: Timestamp::now().to_string(),
        correlation_id,
        kind,
    }
}

async fn wait_for_reconnect(
    service: &DesktopService,
    controls: &mut mpsc::Receiver<ConnectionControl>,
    outbound: &mut mpsc::Receiver<OutboundRequest>,
) -> Option<ConnectionControl> {
    loop {
        tokio::select! {
            () = service.shutdown.cancelled() => return Some(ConnectionControl::Shutdown),
            control = controls.recv() => return control,
            request = outbound.recv() => {
                let request = request?;
                let _ = request.completion.send(Err(
                    "gateway connection is not configured for automatic reconnection".to_owned()
                ));
            }
        }
    }
}

enum DelayOutcome {
    Continue,
    Reconnect,
    Pause,
    Shutdown,
}

async fn wait_delay_or_control(
    service: &DesktopService,
    delay: Duration,
    controls: &mut mpsc::Receiver<ConnectionControl>,
) -> DelayOutcome {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            () = service.shutdown.cancelled() => return DelayOutcome::Shutdown,
            () = &mut sleep => return DelayOutcome::Continue,
            control = controls.recv() => match control {
                Some(ConnectionControl::Reconnect) => return DelayOutcome::Reconnect,
                Some(ConnectionControl::ConfigurationChanged) => return DelayOutcome::Pause,
                Some(ConnectionControl::PublishInventory) => {}
                Some(ConnectionControl::Shutdown) | None => return DelayOutcome::Shutdown,
            }
        }
    }
}

fn next_retry_delay(backoff: &mut ReconnectBackoff) -> Option<Duration> {
    backoff
        .next_delay()
        .ok()
        .flatten()
        .map(|(_attempt, delay)| delay)
}

fn reject_pending(pending: &mut VecDeque<OutboundRequest>, message: &str) {
    let message = sanitize(message);
    while let Some(request) = pending.pop_front() {
        let _ = request.completion.send(Err(message.clone()));
    }
}

fn finish_outbound(outbound: &mut mpsc::Receiver<OutboundRequest>, message: &str) {
    let message = sanitize(message);
    while let Ok(request) = outbound.try_recv() {
        let _ = request.completion.send(Err(message.clone()));
    }
}

#[cfg(test)]
mod tests {
    use oppa_protocol::{AgentMessageKind, Validate};

    use super::envelope;

    #[test]
    fn outbound_envelope_uses_unique_bounded_identity() {
        let first = envelope(
            AgentMessageKind::Heartbeat(oppa_protocol::AgentHeartbeat { uptime_seconds: 1 }),
            Some("server-message".to_owned()),
        );
        let second = envelope(
            AgentMessageKind::Heartbeat(oppa_protocol::AgentHeartbeat { uptime_seconds: 1 }),
            Some("server-message".to_owned()),
        );

        assert_ne!(first.message_id, second.message_id);
        assert!(first.validate().is_ok());
    }
}
