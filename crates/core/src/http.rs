//! Http capability — see /agent-docs/capabilities/http/CONTRACT.md.
//! Outbound HTTP requests (any method, headers, query params, a body),
//! made native-side to escape webview CORS; portable via `reqwest` — same
//! reasoning `storage.rs`/`files.rs` established for `rusqlite`/
//! `std::fs`. There is no per-OS branching here; see
//! agent-docs/capabilities/http/research/WINDOWS.md for why the default
//! (`native-tls`) TLS backend, not `rustls`, is the deliberate choice.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum HttpError {
    /// Malformed URL, or a scheme other than http/https — caught before
    /// any network activity.
    InvalidUrl(String),
    /// No response was received at all: DNS failure, connection
    /// refused, or the request timed out.
    Unavailable(String),
    /// The body is longer than the request's `max_bytes`; reading stopped there.
    TooLarge(String),
    Other(String),
}

impl HttpError {
    pub fn message(&self) -> &str {
        match self {
            HttpError::InvalidUrl(m) | HttpError::Unavailable(m) | HttpError::TooLarge(m) | HttpError::Other(m) => m,
        }
    }
}

/// A response without its body — what `request_bytes` sends ahead of the bytes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseHead {
    pub status: u16,
    pub status_text: String,
    pub ok: bool,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub ok: bool,
    /// Lowercased names; repeated headers joined with ", " (as axios does).
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// The request body, already encoded by the SDK (see packages/sdk/src/http.ts).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum HttpBody {
    /// Sent as-is; `Content-Type: text/plain;charset=utf-8` unless set.
    Text(String),
    /// Serialized JSON; `Content-Type: application/json` unless set.
    Json(String),
    /// `a=1&b=2`; `Content-Type: application/x-www-form-urlencoded` unless set.
    Form(String),
    /// Base64 of raw bytes; `Content-Type: application/octet-stream` unless set.
    Bytes(String),
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub url: String,
    /// Defaults to GET.
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Appended to the URL's query in order, after any it already has.
    #[serde(default)]
    pub params: Vec<(String, String)>,
    pub body: Option<HttpBody>,
    /// Milliseconds; defaults to `DEFAULT_TIMEOUT`.
    pub timeout_ms: Option<u64>,
    /// `request_bytes` only.
    pub max_bytes: Option<u64>,
}

/// Bounds how long a stalled connection can hang the caller when no
/// `timeout` is given.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A plain GET — kept for existing callers of `desktop.http.get(url)`.
pub async fn get(url: &str) -> Result<HttpResponse, HttpError> {
    request(HttpRequest { url: url.to_string(), ..Default::default() }).await
}

/// Resolving does not mean HTTP success — any received response
/// (2xx-5xx) is `Ok`; only a request that never gets a response at all is
/// `Err` (see CONTRACT.md's Errors section).
pub async fn request(request: HttpRequest) -> Result<HttpResponse, HttpError> {
    let (head, response) = send(request).await?;
    let body = response.text().await.map_err(read_error)?;
    Ok(HttpResponse { status: head.status, status_text: head.status_text, ok: head.ok, headers: head.headers, body })
}

/// Like `request`, but the body stays bytes: the result is the head as
/// JSON, prefixed with its length as a big-endian u32, then the body —
/// one buffer for Tauri's raw IPC response, so an image never becomes a
/// JSON number array. Stops with `TooLarge` past `max_bytes`.
pub async fn request_bytes(request: HttpRequest) -> Result<Vec<u8>, HttpError> {
    let max_bytes = request.max_bytes;
    let (head, mut response) = send(request).await?;
    let too_large = |max: u64| HttpError::TooLarge(format!("the response is larger than maxBytes ({max} bytes)"));
    if let (Some(max), Some(length)) = (max_bytes, response.content_length()) {
        if length > max {
            return Err(too_large(max));
        }
    }
    let head = serde_json::to_vec(&head).map_err(|e| HttpError::Other(e.to_string()))?;
    let mut framed = Vec::with_capacity(4 + head.len() + response.content_length().unwrap_or(0) as usize);
    framed.extend_from_slice(&(head.len() as u32).to_be_bytes());
    framed.extend_from_slice(&head);
    let body_start = framed.len();
    while let Some(chunk) = response.chunk().await.map_err(read_error)? {
        if let Some(max) = max_bytes {
            if (framed.len() - body_start + chunk.len()) as u64 > max {
                return Err(too_large(max));
            }
        }
        framed.extend_from_slice(&chunk);
    }
    Ok(framed)
}

fn read_error(e: reqwest::Error) -> HttpError {
    if e.is_timeout() {
        HttpError::Unavailable(e.to_string())
    } else {
        HttpError::Other(e.to_string())
    }
}

async fn send(request: HttpRequest) -> Result<(ResponseHead, reqwest::Response), HttpError> {
    let invalid = |m: String| HttpError::InvalidUrl(m);
    let mut url = reqwest::Url::parse(&request.url).map_err(|e| invalid(e.to_string()))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(invalid(format!("unsupported URL scheme: {}", url.scheme())));
    }
    if !request.params.is_empty() {
        url.query_pairs_mut().extend_pairs(&request.params);
    }
    let method = request.method.as_deref().unwrap_or("GET").to_ascii_uppercase();
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| invalid(format!("invalid HTTP method {method:?}")))?;

    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in &request.headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid(format!("invalid header name {name:?}")))?;
        let value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| invalid(format!("invalid value for header {name}")))?;
        headers.append(name, value);
    }

    let client = reqwest::Client::builder()
        .timeout(request.timeout_ms.map_or(DEFAULT_TIMEOUT, Duration::from_millis))
        .build()
        .map_err(|e| HttpError::Other(e.to_string()))?;
    let mut builder = client.request(method, url);
    if let Some(body) = request.body {
        let (content_type, bytes) = match body {
            HttpBody::Text(text) => ("text/plain;charset=utf-8", text.into_bytes()),
            HttpBody::Json(json) => ("application/json", json.into_bytes()),
            HttpBody::Form(form) => ("application/x-www-form-urlencoded", form.into_bytes()),
            HttpBody::Bytes(base64) => (
                "application/octet-stream",
                decode_base64(&base64).ok_or_else(|| invalid("request body bytes aren't valid base64".to_string()))?,
            ),
        };
        if !headers.contains_key(reqwest::header::CONTENT_TYPE) {
            headers.insert(reqwest::header::CONTENT_TYPE, reqwest::header::HeaderValue::from_static(content_type));
        }
        builder = builder.body(bytes);
    }

    let response = builder.headers(headers).send().await.map_err(|e| {
        if e.is_timeout() || e.is_connect() {
            HttpError::Unavailable(e.to_string())
        } else {
            HttpError::Other(e.to_string())
        }
    })?;

    let status = response.status();
    let mut response_headers: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in response.headers() {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        response_headers
            .entry(name.as_str().to_string())
            .and_modify(|joined| {
                joined.push_str(", ");
                joined.push_str(&value);
            })
            .or_insert(value);
    }
    let head = ResponseHead {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        ok: status.is_success(),
        headers: response_headers,
    };
    Ok((head, response))
}

/// Standard base64 (with or without padding). Small enough not to need a crate.
fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = input.bytes().filter(|c| !c.is_ascii_whitespace()).take_while(|c| *c != b'=').collect();
    if digits.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut buffer = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            buffer |= u32::from(value(*c)?) << (18 - 6 * i);
        }
        out.extend_from_slice(&buffer.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
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

    /// Like `serve_once`, but hands back the raw request it received and
    /// replies with a header the client should see.
    fn serve_capturing() -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            // Read headers, then as much body as Content-Length says.
            loop {
                let n = stream.read(&mut buf).unwrap();
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            tx.send(String::from_utf8_lossy(&raw).to_string()).unwrap();
            let body = r#"{"created":true}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nX-Trace: a\r\nX-Trace: b\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{addr}/items?existing=1"), rx)
    }

    #[tokio::test]
    async fn sends_method_headers_params_and_a_json_body() {
        let (url, received) = serve_capturing();
        let response = request(HttpRequest {
            url,
            method: Some("post".to_string()),
            headers: vec![("Authorization".to_string(), "Bearer t".to_string())],
            params: vec![("q".to_string(), "a b".to_string()), ("tag[]".to_string(), "x".to_string())],
            body: Some(HttpBody::Json(r#"{"name":"n"}"#.to_string())),
            timeout_ms: Some(5_000),
            max_bytes: None,
        })
        .await
        .unwrap();
        let raw = received.recv().unwrap();
        assert!(raw.starts_with("POST /items?existing=1&q=a+b&tag%5B%5D=x HTTP/1.1"), "{raw}");
        assert!(raw.to_ascii_lowercase().contains("authorization: bearer t"));
        assert!(raw.to_ascii_lowercase().contains("content-type: application/json"));
        assert!(raw.ends_with(r#"{"name":"n"}"#));

        assert_eq!(response.status, 201);
        assert_eq!(response.status_text, "Created");
        assert_eq!(response.headers["content-type"], "application/json");
        assert_eq!(response.headers["x-trace"], "a, b");
        assert_eq!(response.body, r#"{"created":true}"#);
    }

    #[tokio::test]
    async fn an_explicit_content_type_wins_and_bytes_arrive_intact() {
        let (url, received) = serve_capturing();
        request(HttpRequest {
            url,
            method: Some("PUT".to_string()),
            headers: vec![("Content-Type".to_string(), "image/png".to_string())],
            body: Some(HttpBody::Bytes("iVBORw0KGgo=".to_string())),
            ..Default::default()
        })
        .await
        .unwrap();
        let raw = received.recv().unwrap();
        assert!(raw.starts_with("PUT "));
        assert!(raw.to_ascii_lowercase().contains("content-type: image/png"));
        assert!(!raw.contains("octet-stream"));
        // The server reads lossily as UTF-8, so the PNG signature's 0x89 shows as U+FFFD.
        assert!(raw.ends_with("\u{FFFD}PNG\r\n\u{1a}\n"), "{raw:?}");
    }

    #[tokio::test]
    async fn rejects_bad_methods_and_headers() {
        let bad_method = HttpRequest { url: "http://127.0.0.1:1/".to_string(), method: Some("NOT A METHOD".to_string()), ..Default::default() };
        assert!(matches!(request(bad_method).await, Err(HttpError::InvalidUrl(_))));
        let bad_header = HttpRequest {
            url: "http://127.0.0.1:1/".to_string(),
            headers: vec![("Bad Header".to_string(), "x".to_string())],
            ..Default::default()
        };
        assert!(matches!(request(bad_header).await, Err(HttpError::InvalidUrl(_))));
    }

    #[test]
    fn decodes_base64() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(decode_base64("").unwrap(), b"");
        assert!(decode_base64("a").is_none());
        assert!(decode_base64("a$==").is_none());
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

    /// Replies to each connection, in order, with the given raw responses.
    fn serve_raw(responses: Vec<Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(&response);
            }
        });
        format!("http://{addr}/")
    }

    fn png_response(headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\n{headers}Connection: close\r\n\r\n").into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn unframe(framed: &[u8]) -> (serde_json::Value, &[u8]) {
        let length = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
        (serde_json::from_slice(&framed[4..4 + length]).unwrap(), &framed[4 + length..])
    }

    const PNG_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00, 0xfe];

    #[tokio::test]
    async fn returns_bytes_intact_after_following_a_redirect() {
        let url = serve_raw(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /image.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            png_response(&format!("Content-Length: {}\r\n", PNG_BYTES.len()), PNG_BYTES),
        ]);
        let framed = request_bytes(HttpRequest { url, max_bytes: Some(1_000), ..Default::default() }).await.unwrap();
        let (head, body) = unframe(&framed);
        assert_eq!(head["status"], 200);
        assert_eq!(head["headers"]["content-type"], "image/png");
        assert_eq!(body, PNG_BYTES);
    }

    #[tokio::test]
    async fn rejects_a_declared_length_over_max_bytes() {
        let url = serve_raw(vec![png_response("Content-Length: 11\r\n", PNG_BYTES)]);
        let result = request_bytes(HttpRequest { url, max_bytes: Some(10), ..Default::default() }).await;
        assert!(matches!(result, Err(HttpError::TooLarge(m)) if m.contains("10 bytes")));
    }

    #[tokio::test]
    async fn stops_reading_an_undeclared_body_past_max_bytes() {
        let url = serve_raw(vec![png_response("", &[7u8; 64 * 1024])]);
        let result = request_bytes(HttpRequest { url, max_bytes: Some(1024), ..Default::default() }).await;
        assert!(matches!(result, Err(HttpError::TooLarge(_))));
    }

}
