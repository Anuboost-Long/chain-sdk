//! Http capability — see /agent-docs/capabilities/http/CONTRACT.md.
//! A single outbound HTTP GET, made portable by `reqwest` — same
//! reasoning `storage.rs`/`files.rs` established for `rusqlite`/
//! `std::fs`. There is no per-OS branching here; see
//! agent-docs/capabilities/http/research/WINDOWS.md for why the default
//! (`native-tls`) TLS backend, not `rustls`, is the deliberate choice.

use std::time::Duration;

use serde::Serialize;

#[derive(Debug)]
pub enum HttpError {
    /// Malformed URL, or a scheme other than http/https — caught before
    /// any network activity.
    InvalidUrl(String),
    /// No response was received at all: DNS failure, connection
    /// refused, or the request timed out.
    Unavailable(String),
    Other(String),
}

impl HttpError {
    pub fn message(&self) -> &str {
        match self {
            HttpError::InvalidUrl(m) | HttpError::Unavailable(m) | HttpError::Other(m) => m,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub ok: bool,
    pub body: String,
}

/// A safety default, not a contract parameter (see CONTRACT.md's
/// Non-goals) — bounds how long a stalled connection can hang the
/// caller.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Fetches `url` with a plain GET. Resolving does not mean HTTP success —
/// any received response (2xx-5xx) is `Ok`; only a request that never
/// gets a response at all is `Err` (see CONTRACT.md's Errors section).
pub async fn get(url: &str) -> Result<HttpResponse, HttpError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(HttpError::InvalidUrl(format!(
            "unsupported URL scheme: {}",
            parsed.scheme()
        )));
    }

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| HttpError::Other(e.to_string()))?;

    let response = client.get(parsed).send().await.map_err(|e| {
        if e.is_timeout() || e.is_connect() {
            HttpError::Unavailable(e.to_string())
        } else {
            HttpError::Other(e.to_string())
        }
    })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| HttpError::Other(e.to_string()))?;

    Ok(HttpResponse {
        status: status.as_u16(),
        ok: status.is_success(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Starts a minimal loopback HTTP/1.1 server that replies once with a
    /// fixed status/body, then stops — enough to exercise the real
    /// request/response path without any internet dependency.
    fn serve_once(status_line: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let status_line = status_line.to_string();
        let body = body.to_string();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn round_trips_a_successful_response() {
        let url = serve_once("HTTP/1.1 200 OK", "<html>hello</html>");
        let response = get(&url).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(response.ok);
        assert_eq!(response.body, "<html>hello</html>");
    }

    #[tokio::test]
    async fn a_non_2xx_response_resolves_rather_than_rejecting() {
        let url = serve_once("HTTP/1.1 404 Not Found", "nope");
        let response = get(&url).await.unwrap();
        assert_eq!(response.status, 404);
        assert!(!response.ok);
        assert_eq!(response.body, "nope");
    }

    #[tokio::test]
    async fn rejects_a_malformed_url() {
        let err = get("not a url").await.unwrap_err();
        assert!(matches!(err, HttpError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn rejects_an_unsupported_scheme() {
        let err = get("ftp://example.com/file").await.unwrap_err();
        assert!(matches!(err, HttpError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn reports_unavailable_when_nothing_is_listening() {
        // Bind then immediately drop, so the port is real but nothing
        // accepts the connection — a reliable, fast, local-only way to
        // trigger connection-refused without touching the network.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let err = get(&format!("http://{addr}/")).await.unwrap_err();
        assert!(matches!(err, HttpError::Unavailable(_)));
    }
}
