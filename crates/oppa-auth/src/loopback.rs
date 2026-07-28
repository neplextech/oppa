use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Instant, timeout_at},
};
use url::Url;

use oppa_platform::SecretValue;

use crate::{AuthError, AuthResult, AuthorizationState};

const MAX_CALLBACK_REQUEST_BYTES: usize = 16 * 1024;

/// Validated one-time authorization response.
pub struct AuthorizationCallback {
    /// One-time authorization code. Callers must not log it.
    pub code: SecretValue,
}

impl std::fmt::Debug for AuthorizationCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AuthorizationCallback { code: [REDACTED] }")
    }
}

/// Ephemeral HTTP callback server bound exclusively to IPv4 loopback.
pub struct LoopbackCallback {
    listener: TcpListener,
    redirect_uri: Url,
    expires_at: Instant,
}

impl LoopbackCallback {
    /// Binds `127.0.0.1` on an operating-system-assigned ephemeral port.
    pub async fn bind(lifetime: Duration) -> AuthResult<Self> {
        if lifetime.is_zero() || lifetime > Duration::from_secs(15 * 60) {
            return Err(AuthError::InvalidConfiguration(
                "callback lifetime must be between 1 nanosecond and 15 minutes".to_owned(),
            ));
        }
        let listener =
            TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await?;
        let address = listener.local_addr()?;
        if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
            return Err(AuthError::Callback(
                "callback listener did not bind exclusively to IPv4 loopback".to_owned(),
            ));
        }
        let redirect_uri = Url::parse(&format!("http://127.0.0.1:{}/callback", address.port()))
            .map_err(|error| AuthError::Callback(error.to_string()))?;
        Ok(Self {
            listener,
            redirect_uri,
            expires_at: Instant::now() + lifetime,
        })
    }

    /// Returns the exact redirect URI to send to the provider.
    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// Accepts one valid callback, then consumes and closes the listener.
    pub async fn wait(
        self,
        expected_state: &AuthorizationState,
    ) -> AuthResult<AuthorizationCallback> {
        loop {
            let (mut stream, peer) = timeout_at(self.expires_at, self.listener.accept())
                .await
                .map_err(|_| AuthError::CallbackTimeout)??;
            if !peer.ip().is_loopback() {
                let _ = send_response(&mut stream, 403, "Forbidden").await;
                continue;
            }
            match timeout_at(self.expires_at, read_request(&mut stream)).await {
                Err(_) => return Err(AuthError::CallbackTimeout),
                Ok(Err(error)) => {
                    let _ = send_response(&mut stream, 400, "Invalid authorization response").await;
                    return Err(error);
                }
                Ok(Ok(RequestOutcome::Ignore)) => {
                    let _ = send_response(&mut stream, 404, "Not Found").await;
                }
                Ok(Ok(RequestOutcome::Callback { code, state })) => {
                    if !expected_state.matches(&state) {
                        let _ =
                            send_response(&mut stream, 400, "Authorization state did not match")
                                .await;
                        return Err(AuthError::InvalidState);
                    }
                    validate_code(&code)?;
                    let _ = send_response(
                        &mut stream,
                        200,
                        "Authorization complete. You may close this window and return to the printer agent.",
                    )
                    .await;
                    return Ok(AuthorizationCallback {
                        code: SecretValue::new(code),
                    });
                }
                Ok(Ok(RequestOutcome::ProviderError { error, description })) => {
                    let _ =
                        send_response(&mut stream, 400, "Authorization was not completed").await;
                    let description = description
                        .map(|value| format!(": {}", value.chars().take(300).collect::<String>()))
                        .unwrap_or_default();
                    return Err(AuthError::Callback(format!(
                        "provider returned {}{}",
                        error.chars().take(100).collect::<String>(),
                        description
                    )));
                }
            }
        }
    }
}

enum RequestOutcome {
    Ignore,
    Callback {
        code: String,
        state: String,
    },
    ProviderError {
        error: String,
        description: Option<String>,
    },
}

async fn read_request(stream: &mut TcpStream) -> AuthResult<RequestOutcome> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Err(AuthError::Callback(
                "callback connection closed before HTTP headers completed".to_owned(),
            ));
        }
        request.extend_from_slice(&chunk[..count]);
        if request.len() > MAX_CALLBACK_REQUEST_BYTES {
            return Err(AuthError::Callback(
                "callback HTTP request exceeded 16 KiB".to_owned(),
            ));
        }
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&request)
        .map_err(|_| AuthError::Callback("callback request was not UTF-8 HTTP".to_owned()))?;
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| AuthError::Callback("callback request line is missing".to_owned()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if method != "GET" || !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err(AuthError::Callback(
            "callback must be an HTTP/1 GET request".to_owned(),
        ));
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|error| AuthError::Callback(format!("invalid callback target: {error}")))?;
    if url.path() != "/callback" {
        return Ok(RequestOutcome::Ignore);
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut description = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" if code.is_none() => code = Some(value.into_owned()),
            "state" if state.is_none() => state = Some(value.into_owned()),
            "error" if error.is_none() => error = Some(value.into_owned()),
            "error_description" if description.is_none() => {
                description = Some(value.into_owned());
            }
            _ => {}
        }
    }
    if let Some(error) = error {
        return Ok(RequestOutcome::ProviderError { error, description });
    }
    let code =
        code.ok_or_else(|| AuthError::Callback("callback omitted authorization code".to_owned()))?;
    let state = state
        .ok_or_else(|| AuthError::Callback("callback omitted authorization state".to_owned()))?;
    Ok(RequestOutcome::Callback { code, state })
}

fn validate_code(code: &str) -> AuthResult<()> {
    if code.is_empty() || code.len() > 4096 || code.chars().any(char::is_control) {
        return Err(AuthError::Callback(
            "authorization code is empty or exceeds safe limits".to_owned(),
        ));
    }
    Ok(())
}

async fn send_response(stream: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Printer Agent Authorization</title><p>{message}</p>"
    );
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpStream;

    use super::*;

    #[tokio::test]
    async fn callback_binds_loopback_and_accepts_one_matching_state() {
        let callback = LoopbackCallback::bind(Duration::from_secs(2))
            .await
            .expect("bind");
        assert_eq!(callback.redirect_uri().host_str(), Some("127.0.0.1"));
        let address = format!(
            "127.0.0.1:{}",
            callback.redirect_uri().port().expect("ephemeral port")
        );
        let state =
            AuthorizationState::from_value("0123456789abcdef0123456789abcdef").expect("state");
        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(address).await.expect("connect");
            stream
                .write_all(
                    b"GET /callback?code=one-time-code&state=0123456789abcdef0123456789abcdef HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                )
                .await
                .expect("send");
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.expect("response");
            String::from_utf8(response).expect("UTF-8 response")
        });
        let result = callback.wait(&state).await.expect("callback");
        assert_eq!(result.code.expose_secret(), "one-time-code");
        let response = client.await.expect("client task");
        assert!(response.contains("200 OK"));
        assert!(response.contains("Printer Agent Authorization"));
        assert!(response.contains("return to the printer agent"));
        assert!(!response.contains("OPPA"));
    }

    #[tokio::test]
    async fn mismatched_state_is_rejected() {
        let callback = LoopbackCallback::bind(Duration::from_secs(2))
            .await
            .expect("bind");
        let address = format!(
            "127.0.0.1:{}",
            callback.redirect_uri().port().expect("ephemeral port")
        );
        let state =
            AuthorizationState::from_value("0123456789abcdef0123456789abcdef").expect("state");
        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(address).await.expect("connect");
            stream
                .write_all(
                    b"GET /callback?code=code&state=ffffffffffffffffffffffffffffffff HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                )
                .await
                .expect("send");
        });
        assert!(matches!(
            callback.wait(&state).await,
            Err(AuthError::InvalidState)
        ));
        client.await.expect("client task");
    }
}
