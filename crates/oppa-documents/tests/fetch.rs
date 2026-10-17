//! Fetcher behaviour against a real loopback HTTP server.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use oppa_auth::AgentKeyManager;
use oppa_documents::{
    DocumentFetchError, DocumentFetcher, HttpDocumentFetcher, sha256_hex, verify_digest,
};
use oppa_platform::{CredentialStore, MemoryCredentialStore, SecretValue};
use oppa_protocol::PdfSource;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const BODY: &[u8] = b"%PDF-1.4 fixture body";

/// One canned HTTP response served for every connection.
#[derive(Clone)]
struct Canned {
    status_line: &'static str,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

async fn serve(response: Canned) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let address = listener.local_addr().expect("local address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = vec![0_u8; 8192];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
            let mut head = format!("HTTP/1.1 {}\r\nConnection: close\r\n", response.status_line);
            for (name, value) in &response.headers {
                head.push_str(name);
                head.push_str(": ");
                head.push_str(value);
                head.push_str("\r\n");
            }
            head.push_str("\r\n");
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&response.body).await;
            let _ = socket.shutdown().await;
        }
    });
    (address, requests)
}

async fn signing_fetcher() -> (HttpDocumentFetcher, Arc<dyn CredentialStore>) {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    let keys = AgentKeyManager::new(Arc::clone(&store));
    let generated = keys
        .generate("server_1")
        .await
        .expect("generate credential");
    let fetcher = HttpDocumentFetcher::new(
        keys,
        generated.credential_ref,
        "agent_1",
        Duration::from_secs(5),
    )
    .expect("fetcher");
    (fetcher, store)
}

fn source(address: SocketAddr, body: &[u8], byte_length: u64) -> PdfSource {
    PdfSource::Url {
        url: format!("http://{address}/v1/documents/doc_1"),
        sha256: sha256_hex(body),
        byte_length,
    }
}

fn ok_response(body: &[u8]) -> Canned {
    Canned {
        status_line: "200 OK",
        headers: vec![
            ("Content-Type", "application/pdf".to_owned()),
            ("Content-Length", body.len().to_string()),
        ],
        body: body.to_vec(),
    }
}

#[tokio::test]
async fn verified_document_is_returned_with_signed_request_headers() {
    let (address, requests) = serve(ok_response(BODY)).await;
    let (fetcher, _store) = signing_fetcher().await;

    let bytes = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect("verified document");
    assert_eq!(bytes, BODY);

    let request = requests
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("one request");
    assert!(request.starts_with("GET /v1/documents/doc_1 HTTP/1.1"));
    let lower = request.to_ascii_lowercase();
    assert!(lower.contains("x-openprinter-agent-id: agent_1"));
    assert!(lower.contains("x-openprinter-job-id: job_1"));
    assert!(lower.contains("x-openprinter-timestamp: "));
    assert!(lower.contains("x-openprinter-signature: "));
    assert!(
        !lower.contains("authorization:"),
        "no bearer secret may be sent"
    );
}

#[tokio::test]
async fn body_that_fails_the_declared_digest_is_rejected() {
    let served = b"%PDF-1.4 tampered body".to_vec();
    let (address, _requests) = serve(ok_response(&served)).await;
    let (fetcher, _store) = signing_fetcher().await;

    let error = fetcher
        .fetch_pdf(
            "job_1",
            &PdfSource::Url {
                url: format!("http://{address}/v1/documents/doc_1"),
                sha256: sha256_hex(BODY),
                byte_length: served.len() as u64,
            },
        )
        .await
        .expect_err("digest mismatch must fail");
    assert!(
        matches!(error, DocumentFetchError::IntegrityMismatch),
        "{error:?}"
    );
    assert!(!error.retryable());
}

#[tokio::test]
async fn body_longer_than_the_declared_length_is_rejected() {
    let oversized = [BODY, b" plus more bytes"].concat();
    let (address, _requests) = serve(Canned {
        status_line: "200 OK",
        headers: vec![],
        body: oversized,
    })
    .await;
    let (fetcher, _store) = signing_fetcher().await;

    let error = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect_err("oversized body must fail");
    assert!(
        matches!(error, DocumentFetchError::LengthMismatch { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn missing_documents_are_permanent_failures() {
    let (address, _requests) = serve(Canned {
        status_line: "404 Not Found",
        headers: vec![],
        body: Vec::new(),
    })
    .await;
    let (fetcher, _store) = signing_fetcher().await;

    let error = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect_err("404 must fail");
    assert!(matches!(
        error,
        DocumentFetchError::NotFound { status: 404 }
    ));
    assert!(!error.retryable());
}

#[tokio::test]
async fn server_errors_are_retryable() {
    let (address, _requests) = serve(Canned {
        status_line: "503 Service Unavailable",
        headers: vec![],
        body: Vec::new(),
    })
    .await;
    let (fetcher, _store) = signing_fetcher().await;

    let error = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect_err("503 must fail");
    assert!(error.retryable());
    assert_eq!(error.code(), "document.fetch_failed");
}

#[tokio::test]
async fn redirects_are_refused_rather_than_followed() {
    let (address, _requests) = serve(Canned {
        status_line: "302 Found",
        headers: vec![("Location", "http://127.0.0.1:9/elsewhere".to_owned())],
        body: Vec::new(),
    })
    .await;
    let (fetcher, _store) = signing_fetcher().await;

    let error = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect_err("redirect must not be followed");
    assert!(
        matches!(error, DocumentFetchError::Refused { status: 302 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn invalid_references_fail_before_any_network_access() {
    let (fetcher, _store) = signing_fetcher().await;
    let error = fetcher
        .fetch_pdf(
            "job_1",
            &PdfSource::Url {
                url: "http://documents.example.com/doc".to_owned(),
                sha256: sha256_hex(BODY),
                byte_length: 4,
            },
        )
        .await
        .expect_err("remote plain HTTP is not allowed");
    assert!(matches!(error, DocumentFetchError::InvalidSource(_)));
}

#[tokio::test]
async fn authorization_failures_never_send_a_request() {
    let (address, requests) = serve(ok_response(BODY)).await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    store
        .set("oppa-ed25519-malformed", SecretValue::new("not-base64!"))
        .await
        .expect("store malformed credential");
    let fetcher = HttpDocumentFetcher::new(
        AgentKeyManager::new(store),
        "oppa-ed25519-malformed",
        "agent_1",
        Duration::from_secs(5),
    )
    .expect("fetcher");

    let error = fetcher
        .fetch_pdf("job_1", &source(address, BODY, BODY.len() as u64))
        .await
        .expect_err("a malformed credential must not authorize a download");
    assert!(matches!(error, DocumentFetchError::Authorization(_)));
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn digest_helper_accepts_only_the_declared_bytes() {
    verify_digest(BODY, &sha256_hex(BODY)).expect("matching digest");
    assert!(verify_digest(b"other", &sha256_hex(BODY)).is_err());
}
