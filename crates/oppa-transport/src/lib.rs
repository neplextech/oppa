//! TLS WebSocket transport for the OpenPrinter agent protocol.
//!
//! Credentials are carried only in an HTTP `Authorization` header and retained
//! in a zeroizing value. Protocol encoding/decoding remains centralized in
//! `oppa-protocol`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{collections::VecDeque, time::Duration};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use oppa_platform::SecretValue;
use oppa_protocol::{
    AgentMessage, AgentMessageKind, MAX_WIRE_MESSAGE_BYTES, ProtocolError, ServerMessage,
    decode_server_message, encode_agent_message,
};
use rand::Rng;
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::watch,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        Error as WebSocketError, Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
        protocol::WebSocketConfig,
    },
};
use tokio_util::sync::CancellationToken;
use url::Url;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Default maximum number of application messages buffered while disconnected.
pub const DEFAULT_OUTBOUND_BUFFER_CAPACITY: usize = 256;

/// WebSocket connection settings.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Compile-time configured gateway URL.
    pub endpoint: Url,
    /// Deadline for the WebSocket and TLS handshake.
    pub connect_timeout: Duration,
    /// Maximum interval without an application or WebSocket frame.
    pub idle_timeout: Duration,
    /// Maximum queued outbound application messages.
    pub outbound_buffer_capacity: usize,
}

impl TransportConfig {
    /// Validates TLS and bounded timing requirements.
    pub fn validate(&self) -> TransportResult<()> {
        let loopback = matches!(
            self.endpoint.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        );
        if self.endpoint.scheme() != "wss" && !(self.endpoint.scheme() == "ws" && loopback) {
            return Err(TransportError::InvalidConfiguration(
                "gateway must use WSS except for loopback development".to_owned(),
            ));
        }
        if !self.endpoint.username().is_empty()
            || self.endpoint.password().is_some()
            || self.endpoint.fragment().is_some()
        {
            return Err(TransportError::InvalidConfiguration(
                "gateway URL must not contain credentials or a fragment".to_owned(),
            ));
        }
        if self.connect_timeout.is_zero()
            || self.idle_timeout.is_zero()
            || self.outbound_buffer_capacity == 0
        {
            return Err(TransportError::InvalidConfiguration(
                "timeouts and outbound buffer capacity must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Observable transport lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// No socket is active.
    Disconnected,
    /// A socket and TLS handshake are in progress.
    Connecting,
    /// A WebSocket is active.
    Connected,
    /// A reconnect attempt is waiting for its bounded delay.
    BackingOff {
        /// One-based failed-attempt count.
        attempt: u32,
        /// Selected delay including jitter.
        delay: Duration,
    },
    /// Graceful socket shutdown is in progress.
    Closing,
}

/// Bounded exponential reconnect policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackoffPolicy {
    /// Delay after the first failed attempt.
    pub initial_delay: Duration,
    /// Upper bound before jitter.
    pub maximum_delay: Duration,
    /// Exponential multiplier, at least 1.
    pub multiplier: f64,
    /// Symmetric jitter fraction in the range 0 to 1.
    pub jitter_fraction: f64,
    /// Optional maximum attempts before returning the last error.
    pub maximum_attempts: Option<u32>,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            maximum_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_fraction: 0.2,
            maximum_attempts: None,
        }
    }
}

impl BackoffPolicy {
    /// Validates bounds.
    pub fn validate(&self) -> TransportResult<()> {
        if self.initial_delay.is_zero()
            || self.maximum_delay < self.initial_delay
            || !self.multiplier.is_finite()
            || self.multiplier < 1.0
            || !self.jitter_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.jitter_fraction)
            || self.maximum_attempts == Some(0)
        {
            return Err(TransportError::InvalidConfiguration(
                "invalid reconnect backoff policy".to_owned(),
            ));
        }
        Ok(())
    }

    /// Computes a delay from a deterministic jitter sample in `[-1, 1]`.
    ///
    /// This public deterministic form makes backoff behavior directly testable.
    pub fn delay_for(&self, attempt: u32, jitter_sample: f64) -> TransportResult<Duration> {
        self.validate()?;
        if attempt == 0 || !(-1.0..=1.0).contains(&jitter_sample) {
            return Err(TransportError::InvalidConfiguration(
                "attempt must be positive and jitter sample must be between -1 and 1".to_owned(),
            ));
        }
        let exponent = i32::try_from(attempt.saturating_sub(1).min(63)).unwrap_or(63);
        let base = self
            .initial_delay
            .as_secs_f64()
            .mul_add(self.multiplier.powi(exponent), 0.0)
            .min(self.maximum_delay.as_secs_f64());
        let factor = 1.0 + self.jitter_fraction * jitter_sample;
        Ok(Duration::from_secs_f64(
            (base * factor)
                .max(0.001)
                .min(self.maximum_delay.as_secs_f64()),
        ))
    }
}

/// Reconnect attempt counter with reset-on-success behavior.
#[derive(Debug, Clone)]
pub struct ReconnectBackoff {
    policy: BackoffPolicy,
    attempts: u32,
}

impl ReconnectBackoff {
    /// Creates a validated controller.
    pub fn new(policy: BackoffPolicy) -> TransportResult<Self> {
        policy.validate()?;
        Ok(Self {
            policy,
            attempts: 0,
        })
    }

    /// Returns the next attempt number and bounded jittered delay.
    pub fn next_delay(&mut self) -> TransportResult<Option<(u32, Duration)>> {
        if self
            .policy
            .maximum_attempts
            .is_some_and(|maximum| self.attempts >= maximum)
        {
            return Ok(None);
        }
        self.attempts = self.attempts.saturating_add(1);
        let jitter = rand::rng().random_range(-1.0..=1.0);
        Ok(Some((
            self.attempts,
            self.policy.delay_for(self.attempts, jitter)?,
        )))
    }

    /// Clears the failed-attempt counter after a successful connection.
    pub fn reset(&mut self) {
        self.attempts = 0;
    }

    /// Returns the current failed-attempt count.
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// Agent transport operations independent of Tauri.
#[async_trait]
pub trait AgentTransport: Send {
    /// Establishes and authenticates a WebSocket connection.
    async fn connect(&mut self) -> TransportResult<()>;

    /// Sends one validated agent message.
    async fn send(&mut self, message: AgentMessage) -> TransportResult<()>;

    /// Receives and validates the next server application message.
    async fn receive(&mut self) -> TransportResult<ServerMessage>;

    /// Gracefully closes the socket.
    async fn close(&mut self) -> TransportResult<()>;

    /// Returns the current lifecycle state.
    fn state(&self) -> ConnectionState;
}

/// Production WebSocket-over-TLS implementation.
pub struct WebSocketTransport {
    config: TransportConfig,
    credential: Option<SecretValue>,
    socket: Option<Socket>,
    state: ConnectionState,
    state_sender: watch::Sender<ConnectionState>,
    outbound: VecDeque<AgentMessage>,
    hello_sent: bool,
}

impl WebSocketTransport {
    /// Creates a disconnected transport.
    pub fn new(config: TransportConfig) -> TransportResult<Self> {
        config.validate()?;
        let (state_sender, _) = watch::channel(ConnectionState::Disconnected);
        Ok(Self {
            config,
            credential: None,
            socket: None,
            state: ConnectionState::Disconnected,
            state_sender,
            outbound: VecDeque::new(),
            hello_sent: false,
        })
    }

    /// Replaces the in-memory bearer credential used on the next connection.
    pub fn set_bearer_token(&mut self, credential: SecretValue) {
        self.credential = Some(credential);
    }

    /// Drops the in-memory bearer credential.
    pub fn clear_bearer_token(&mut self) {
        self.credential = None;
    }

    /// Subscribes to transport state changes.
    #[must_use]
    pub fn subscribe_state(&self) -> watch::Receiver<ConnectionState> {
        self.state_sender.subscribe()
    }

    /// Explicitly buffers a validated outbound message for the next connection.
    ///
    /// Durable job state remains in `oppa-storage`; this small memory buffer is
    /// only for bounded protocol events such as inventory changes.
    pub fn buffer(&mut self, message: AgentMessage) -> TransportResult<()> {
        encode_agent_message(&message)?;
        if matches!(&message.kind, AgentMessageKind::Hello(_)) {
            return Err(TransportError::HelloCannotBeBuffered);
        }
        if self.outbound.len() >= self.config.outbound_buffer_capacity {
            return Err(TransportError::OutboundBufferFull {
                capacity: self.config.outbound_buffer_capacity,
            });
        }
        self.outbound.push_back(message);
        Ok(())
    }

    /// Returns the current buffered message count.
    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.outbound.len()
    }

    /// Connects repeatedly with bounded exponential backoff until success,
    /// cancellation, a non-retryable error, or the attempt limit.
    pub async fn connect_with_backoff(
        &mut self,
        backoff: &mut ReconnectBackoff,
        cancellation: &CancellationToken,
    ) -> TransportResult<()> {
        loop {
            match self.connect().await {
                Ok(()) => {
                    backoff.reset();
                    return Ok(());
                }
                Err(error) if !error.is_retryable() => return Err(error),
                Err(last_error) => {
                    let Some((attempt, delay)) = backoff.next_delay()? else {
                        return Err(last_error);
                    };
                    self.set_state(ConnectionState::BackingOff { attempt, delay });
                    tokio::select! {
                        () = cancellation.cancelled() => {
                            self.set_state(ConnectionState::Disconnected);
                            return Err(TransportError::Cancelled);
                        }
                        () = sleep(delay) => {}
                    }
                }
            }
        }
    }

    fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
        self.state_sender.send_replace(state);
    }

    async fn send_encoded(&mut self, message: &AgentMessage) -> TransportResult<()> {
        let bytes = encode_agent_message(message)?;
        let text = String::from_utf8(bytes)
            .map_err(|error| TransportError::ProtocolEncoding(error.to_string()))?;
        let socket = self.socket.as_mut().ok_or(TransportError::NotConnected)?;
        socket
            .send(Message::Text(text.into()))
            .await
            .map_err(map_websocket_error)
    }

    async fn flush_buffer(&mut self) -> TransportResult<()> {
        while let Some(message) = self.outbound.front().cloned() {
            self.send_encoded(&message).await?;
            self.outbound.pop_front();
        }
        Ok(())
    }

    fn disconnect(&mut self) {
        self.socket = None;
        self.hello_sent = false;
        self.set_state(ConnectionState::Disconnected);
    }
}

#[async_trait]
impl AgentTransport for WebSocketTransport {
    async fn connect(&mut self) -> TransportResult<()> {
        if self.socket.is_some() {
            return Err(TransportError::AlreadyConnected);
        }
        let credential = self
            .credential
            .as_ref()
            .ok_or(TransportError::MissingCredential)?;
        let mut request = self
            .config
            .endpoint
            .as_str()
            .into_client_request()
            .map_err(|error| TransportError::InvalidConfiguration(error.to_string()))?;
        let mut header = HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret()))
            .map_err(|_| TransportError::InvalidCredential)?;
        header.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, header);
        self.set_state(ConnectionState::Connecting);
        let connection = timeout(
            self.config.connect_timeout,
            connect_async_with_config(request, Some(websocket_config()), false),
        )
        .await;
        match connection {
            Err(_) => {
                self.set_state(ConnectionState::Disconnected);
                Err(TransportError::ConnectTimeout(self.config.connect_timeout))
            }
            Ok(Err(error)) => {
                self.set_state(ConnectionState::Disconnected);
                Err(map_websocket_error(error))
            }
            Ok(Ok((socket, _response))) => {
                self.socket = Some(socket);
                self.hello_sent = false;
                self.set_state(ConnectionState::Connected);
                Ok(())
            }
        }
    }

    async fn send(&mut self, message: AgentMessage) -> TransportResult<()> {
        if self.state != ConnectionState::Connected {
            return Err(TransportError::NotConnected);
        }
        let is_hello = matches!(&message.kind, AgentMessageKind::Hello(_));
        if !self.hello_sent && !is_hello {
            return Err(TransportError::HelloRequired);
        }
        if self.hello_sent && is_hello {
            return Err(TransportError::HelloAlreadySent);
        }

        let result = match self.send_encoded(&message).await {
            Ok(()) if is_hello => {
                self.hello_sent = true;
                self.flush_buffer().await
            }
            other => other,
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    async fn receive(&mut self) -> TransportResult<ServerMessage> {
        if self.state != ConnectionState::Connected {
            return Err(TransportError::NotConnected);
        }
        if !self.hello_sent {
            return Err(TransportError::HelloRequired);
        }
        loop {
            let frame = {
                let socket = self.socket.as_mut().ok_or(TransportError::NotConnected)?;
                timeout(self.config.idle_timeout, socket.next()).await
            };
            let frame = match frame {
                Err(_) => {
                    self.disconnect();
                    return Err(TransportError::IdleTimeout(self.config.idle_timeout));
                }
                Ok(None) => {
                    self.disconnect();
                    return Err(TransportError::Closed { reason: None });
                }
                Ok(Some(Err(error))) => {
                    self.disconnect();
                    return Err(map_websocket_error(error));
                }
                Ok(Some(Ok(frame))) => frame,
            };
            match frame {
                Message::Text(text) => match decode_server_message(text.as_bytes()) {
                    Ok(message) => return Ok(message),
                    Err(error) => {
                        self.disconnect();
                        return Err(TransportError::Protocol(error));
                    }
                },
                Message::Binary(_) => {
                    self.disconnect();
                    return Err(TransportError::UnexpectedFrame("binary"));
                }
                Message::Ping(payload) => {
                    let socket = self.socket.as_mut().ok_or(TransportError::NotConnected)?;
                    if let Err(error) = socket.send(Message::Pong(payload)).await {
                        self.disconnect();
                        return Err(map_websocket_error(error));
                    }
                }
                Message::Pong(_) => {}
                Message::Close(frame) => {
                    let reason =
                        frame.map(|frame| frame.reason.chars().take(500).collect::<String>());
                    self.disconnect();
                    return Err(TransportError::Closed { reason });
                }
                Message::Frame(_) => {}
            }
        }
    }

    async fn close(&mut self) -> TransportResult<()> {
        let Some(mut socket) = self.socket.take() else {
            self.hello_sent = false;
            self.set_state(ConnectionState::Disconnected);
            return Ok(());
        };
        self.set_state(ConnectionState::Closing);
        let result = timeout(self.config.connect_timeout, socket.close(None)).await;
        self.hello_sent = false;
        self.set_state(ConnectionState::Disconnected);
        match result {
            Err(_) => Err(TransportError::CloseTimeout(self.config.connect_timeout)),
            Ok(Err(error)) => Err(map_websocket_error(error)),
            Ok(Ok(())) => Ok(()),
        }
    }

    fn state(&self) -> ConnectionState {
        self.state
    }
}

/// WebSocket transport failures.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Settings violated TLS or bounded-resource requirements.
    #[error("invalid transport configuration: {0}")]
    InvalidConfiguration(String),
    /// Connection was attempted without an in-memory bearer token.
    #[error("transport bearer credential is missing")]
    MissingCredential,
    /// Credential could not be encoded as an HTTP header.
    #[error("transport bearer credential contains invalid header bytes")]
    InvalidCredential,
    /// A socket was already active.
    #[error("transport is already connected")]
    AlreadyConnected,
    /// An operation requires an active connection.
    #[error("transport is not connected")]
    NotConnected,
    /// TLS/WebSocket connection exceeded its deadline.
    #[error("WebSocket connection timed out after {0:?}")]
    ConnectTimeout(Duration),
    /// No frame arrived within the heartbeat/idle deadline.
    #[error("WebSocket idle timeout elapsed after {0:?}")]
    IdleTimeout(Duration),
    /// Graceful close exceeded its deadline.
    #[error("WebSocket close timed out after {0:?}")]
    CloseTimeout(Duration),
    /// WebSocket I/O or handshake failed.
    #[error("WebSocket transport failed: {0}")]
    WebSocket(String),
    /// The gateway rejected the bearer credential.
    #[error("gateway rejected transport authentication with HTTP {status}")]
    AuthenticationRejected {
        /// HTTP status, normally 401 or 403.
        status: u16,
    },
    /// Peer closed the connection.
    #[error("WebSocket peer closed the connection")]
    Closed {
        /// Sanitized close reason supplied by the peer.
        reason: Option<String>,
    },
    /// Peer sent an undocumented frame family.
    #[error("unexpected WebSocket {0} frame")]
    UnexpectedFrame(&'static str),
    /// Protocol validation failed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Valid protocol output was unexpectedly not UTF-8.
    #[error("protocol text encoding failed: {0}")]
    ProtocolEncoding(String),
    /// In-memory event buffer reached its explicit bound.
    #[error("outbound transport buffer is full (capacity {capacity})")]
    OutboundBufferFull {
        /// Configured capacity.
        capacity: usize,
    },
    /// A protocol event was sent or received before the mandatory hello.
    #[error("agent.hello must be the first application message on every connection")]
    HelloRequired,
    /// A second hello was attempted on the same connection.
    #[error("agent.hello was already sent on this connection")]
    HelloAlreadySent,
    /// Handshake messages cannot enter the reconnect event buffer.
    #[error("agent.hello cannot be buffered; send it explicitly after every connection")]
    HelloCannotBeBuffered,
    /// Reconnect waiting was cancelled.
    #[error("transport operation was cancelled")]
    Cancelled,
}

impl TransportError {
    /// Returns whether reconnecting may succeed without new credentials or
    /// configuration.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectTimeout(_)
                | Self::IdleTimeout(_)
                | Self::CloseTimeout(_)
                | Self::WebSocket(_)
                | Self::Closed { .. }
        )
    }
}

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_WIRE_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_WIRE_MESSAGE_BYTES))
}

fn map_websocket_error(error: WebSocketError) -> TransportError {
    if let WebSocketError::Http(response) = &error {
        let status = response.status().as_u16();
        if matches!(status, 401 | 403) {
            return TransportError::AuthenticationRejected { status };
        }
    }
    TransportError::WebSocket(error.to_string())
}

/// Result alias for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

#[cfg(test)]
mod tests {
    use tokio::{net::TcpListener, sync::oneshot};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;

    fn config(endpoint: Url) -> TransportConfig {
        TransportConfig {
            endpoint,
            connect_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(2),
            outbound_buffer_capacity: 2,
        }
    }

    fn hello() -> AgentMessage {
        oppa_protocol::decode_agent_message(include_bytes!(
            "../../../protocol/fixtures/agent/agent-hello.json"
        ))
        .expect("hello fixture")
    }

    fn inventory_changed() -> AgentMessage {
        oppa_protocol::decode_agent_message(include_bytes!(
            "../../../protocol/fixtures/agent/printer-inventory-changed.json"
        ))
        .expect("inventory fixture")
    }

    #[test]
    fn production_gateway_requires_tls() {
        let remote = config(Url::parse("ws://example.com/agent").expect("URL"));
        assert!(matches!(
            remote.validate(),
            Err(TransportError::InvalidConfiguration(_))
        ));
        let local = config(Url::parse("ws://127.0.0.1:3000/agent").expect("URL"));
        assert!(local.validate().is_ok());
    }

    #[test]
    fn exponential_backoff_is_bounded_and_resettable() {
        let policy = BackoffPolicy {
            initial_delay: Duration::from_secs(1),
            maximum_delay: Duration::from_secs(5),
            multiplier: 2.0,
            jitter_fraction: 0.0,
            maximum_attempts: Some(3),
        };
        assert_eq!(
            policy.delay_for(1, 0.0).expect("first"),
            Duration::from_secs(1)
        );
        assert_eq!(
            policy.delay_for(4, 0.0).expect("bounded"),
            Duration::from_secs(5)
        );
        let mut backoff = ReconnectBackoff::new(policy).expect("policy");
        assert_eq!(
            backoff.next_delay().expect("attempt").expect("allowed").0,
            1
        );
        backoff.reset();
        assert_eq!(backoff.attempts(), 0);
    }

    #[tokio::test]
    async fn websocket_sends_and_receives_validated_protocol_messages() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("WebSocket accept");
            let inbound = socket.next().await.expect("frame").expect("valid frame");
            let Message::Text(text) = inbound else {
                panic!("expected text frame");
            };
            oppa_protocol::decode_agent_message(text.as_bytes()).expect("valid agent message");
            socket
                .send(Message::Text(
                    include_str!("../../../protocol/fixtures/server/server-hello.json")
                        .trim()
                        .to_owned()
                        .into(),
                ))
                .await
                .expect("send server hello");
        });
        let endpoint =
            Url::parse(&format!("ws://127.0.0.1:{}/agent", address.port())).expect("endpoint");
        let mut transport = WebSocketTransport::new(config(endpoint)).expect("transport");
        transport.set_bearer_token(SecretValue::new("test-token"));
        transport.connect().await.expect("connect");
        transport.send(hello()).await.expect("send");
        let message = transport.receive().await.expect("receive");
        assert!(matches!(
            message.kind,
            oppa_protocol::ServerMessageKind::Hello(_)
        ));
        server.await.expect("server task");
    }

    #[test]
    fn protocol_buffer_is_bounded() {
        let mut transport = WebSocketTransport::new(config(
            Url::parse("ws://127.0.0.1:3000/agent").expect("endpoint"),
        ))
        .expect("transport");
        let event = inventory_changed();
        transport.buffer(event.clone()).expect("first");
        transport.buffer(event.clone()).expect("second");
        assert!(matches!(
            transport.buffer(event),
            Err(TransportError::OutboundBufferFull { capacity: 2 })
        ));
        assert!(matches!(
            transport.buffer(hello()),
            Err(TransportError::HelloCannotBeBuffered)
        ));
    }

    #[tokio::test]
    async fn reconnect_flushes_buffer_only_after_explicit_hello() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let (quiet_tx, quiet_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("WebSocket accept");
            assert!(
                timeout(Duration::from_millis(100), socket.next())
                    .await
                    .is_err(),
                "a buffered event was sent before agent.hello"
            );
            quiet_tx.send(()).expect("notify quiet socket");

            let first = socket.next().await.expect("hello frame").expect("hello");
            let second = socket.next().await.expect("event frame").expect("event");
            [first, second].map(|frame| {
                let Message::Text(text) = frame else {
                    panic!("expected text frame");
                };
                oppa_protocol::decode_agent_message(text.as_bytes())
                    .expect("valid agent message")
                    .message_type()
            })
        });

        let endpoint =
            Url::parse(&format!("ws://127.0.0.1:{}/agent", address.port())).expect("endpoint");
        let mut transport = WebSocketTransport::new(config(endpoint)).expect("transport");
        transport.set_bearer_token(SecretValue::new("test-token"));
        let event = inventory_changed();
        transport.buffer(event.clone()).expect("buffer event");
        transport.connect().await.expect("connect");
        quiet_rx.await.expect("server quiet check");
        assert!(matches!(
            transport.send(event).await,
            Err(TransportError::HelloRequired)
        ));
        transport.send(hello()).await.expect("send hello and flush");
        assert_eq!(transport.buffered_len(), 0);

        assert_eq!(
            server.await.expect("server task"),
            ["agent.hello", "agent.printer_inventory_changed"]
        );
    }

    #[test]
    fn websocket_allocation_limits_match_the_protocol_limit() {
        let config = websocket_config();
        assert_eq!(
            config.max_message_size,
            Some(oppa_protocol::MAX_WIRE_MESSAGE_BYTES)
        );
        assert_eq!(
            config.max_frame_size,
            Some(oppa_protocol::MAX_WIRE_MESSAGE_BYTES)
        );
    }

    #[tokio::test]
    async fn oversized_websocket_frame_is_rejected_before_protocol_decoding() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("WebSocket accept");
            socket.next().await.expect("hello frame").expect("hello");
            let send_result = socket
                .send(Message::Text(
                    "x".repeat(oppa_protocol::MAX_WIRE_MESSAGE_BYTES + 1).into(),
                ))
                .await;
            assert!(
                send_result.is_ok() || matches!(&send_result, Err(WebSocketError::Io(_))),
                "unexpected oversized-frame send result: {send_result:?}"
            );
        });

        let endpoint =
            Url::parse(&format!("ws://127.0.0.1:{}/agent", address.port())).expect("endpoint");
        let mut transport = WebSocketTransport::new(config(endpoint)).expect("transport");
        transport.set_bearer_token(SecretValue::new("test-token"));
        transport.connect().await.expect("connect");
        transport.send(hello()).await.expect("send hello");
        assert!(matches!(
            transport.receive().await,
            Err(TransportError::WebSocket(_))
        ));
        assert_eq!(transport.state(), ConnectionState::Disconnected);
        server.await.expect("server task");
    }

    #[test]
    fn authentication_rejection_is_not_retried() {
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(401)
            .body(Some(Vec::new()))
            .expect("HTTP response");
        let error = map_websocket_error(WebSocketError::Http(Box::new(response)));
        assert!(matches!(
            &error,
            TransportError::AuthenticationRejected { status: 401 }
        ));
        assert!(!error.is_retryable());
    }
}
