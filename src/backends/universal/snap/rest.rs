use serde_json::Value;
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    io::{Read, Write},
    time::Duration,
};

#[cfg_attr(not(unix), allow(dead_code))]
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SnapdResponse {
    pub http_status: u16,
    pub status_code: u16,
    pub response_type: String,
    pub result: Value,
    pub change: Option<String>,
    pub raw_body: String,
}

#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug)]
pub enum SnapdRestError {
    SocketMissing(PathBuf),
    PermissionDenied {
        path: PathBuf,
        reason: String,
    },
    ConnectionFailed {
        path: PathBuf,
        reason: String,
    },
    #[allow(dead_code)]
    UnsupportedPlatform,
    UnsupportedEndpoint {
        path: String,
        reason: String,
    },
    UnrecognizedResponse(String),
    Daemon {
        status_code: u16,
        kind: Option<String>,
        message: String,
        raw_body: String,
    },
    Io(std::io::Error),
}

impl SnapdRestError {
    pub fn allows_cli_fallback(&self) -> bool {
        matches!(
            self,
            Self::SocketMissing(_)
                | Self::PermissionDenied { .. }
                | Self::ConnectionFailed { .. }
                | Self::UnsupportedPlatform
                | Self::UnsupportedEndpoint { .. }
                | Self::UnrecognizedResponse(_)
        )
    }

    pub fn fallback_reason(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for SnapdRestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SocketMissing(path) => {
                write!(formatter, "snapd socket does not exist: {}", path.display())
            }
            Self::PermissionDenied { path, reason } => write!(
                formatter,
                "snapd socket access denied at {}: {reason}",
                path.display()
            ),
            Self::ConnectionFailed { path, reason } => write!(
                formatter,
                "could not connect to snapd socket {}: {reason}",
                path.display()
            ),
            Self::UnsupportedPlatform => write!(
                formatter,
                "snapd Unix sockets are unsupported on this platform"
            ),
            Self::UnsupportedEndpoint { path, reason } => {
                write!(formatter, "snapd endpoint {path} is unsupported: {reason}")
            }
            Self::UnrecognizedResponse(reason) => {
                write!(
                    formatter,
                    "snapd returned an unrecognized response: {reason}"
                )
            }
            Self::Daemon {
                status_code,
                kind,
                message,
                ..
            } => {
                write!(formatter, "snapd returned {status_code}")?;
                if let Some(kind) = kind {
                    write!(formatter, " ({kind})")?;
                }
                write!(formatter, ": {message}")
            }
            Self::Io(error) => write!(formatter, "snapd I/O error: {error}"),
        }
    }
}

impl std::error::Error for SnapdRestError {}

pub struct SnapdClient {
    socket_path: PathBuf,
    #[cfg(unix)]
    timeout: Duration,
}

impl SnapdClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            #[cfg(unix)]
            timeout: Duration::from_secs(15),
        }
    }

    #[cfg(all(test, unix))]
    pub fn with_timeout(socket_path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout,
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn get(&self, path: &str) -> Result<SnapdResponse, SnapdRestError> {
        self.request("GET", path, None)
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<SnapdResponse, SnapdRestError> {
        let body = serde_json::to_vec(body)
            .map_err(|error| SnapdRestError::UnrecognizedResponse(error.to_string()))?;
        self.request("POST", path, Some(&body))
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<SnapdResponse, SnapdRestError> {
        if !path.starts_with('/') || path.contains('\r') || path.contains('\n') {
            return Err(SnapdRestError::UnrecognizedResponse(
                "invalid request path".to_owned(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::net::UnixStream;
            if !self.socket_path.exists() {
                return Err(SnapdRestError::SocketMissing(self.socket_path.clone()));
            }
            let mut stream = UnixStream::connect(&self.socket_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    SnapdRestError::PermissionDenied {
                        path: self.socket_path.clone(),
                        reason: error.to_string(),
                    }
                } else {
                    SnapdRestError::ConnectionFailed {
                        path: self.socket_path.clone(),
                        reason: error.to_string(),
                    }
                }
            })?;
            stream
                .set_read_timeout(Some(self.timeout))
                .map_err(SnapdRestError::Io)?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(SnapdRestError::Io)?;
            let body = body.unwrap_or_default();
            let request = format!(
                "{method} {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream
                .write_all(request.as_bytes())
                .and_then(|_| stream.write_all(body))
                .and_then(|_| stream.flush())
                .map_err(SnapdRestError::Io)?;
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let count = stream.read(&mut chunk).map_err(SnapdRestError::Io)?;
                if count == 0 {
                    break;
                }
                if bytes.len().saturating_add(count) > MAX_RESPONSE_BYTES {
                    return Err(SnapdRestError::UnrecognizedResponse(
                        "response exceeded the 8 MiB safety limit".to_owned(),
                    ));
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            parse_http_response(&bytes, path)
        }
        #[cfg(not(unix))]
        {
            let _ = (method, path, body);
            Err(SnapdRestError::UnsupportedPlatform)
        }
    }
}

pub fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg_attr(not(unix), allow(dead_code))]
fn parse_http_response(bytes: &[u8], request_path: &str) -> Result<SnapdResponse, SnapdRestError> {
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| SnapdRestError::UnrecognizedResponse("missing HTTP headers".to_owned()))?;
    let headers = String::from_utf8_lossy(&bytes[..separator]);
    let mut lines = headers.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| SnapdRestError::UnrecognizedResponse("missing HTTP status".to_owned()))?;
    let http_status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| SnapdRestError::UnrecognizedResponse("invalid HTTP status".to_owned()))?;
    let chunked = lines.any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        })
    });
    let raw_body = if chunked {
        decode_chunked(&bytes[separator + 4..])?
    } else {
        bytes[separator + 4..].to_vec()
    };
    let raw_body = String::from_utf8(raw_body)
        .map_err(|error| SnapdRestError::UnrecognizedResponse(error.to_string()))?;
    let envelope: Value = serde_json::from_str(&raw_body)
        .map_err(|error| SnapdRestError::UnrecognizedResponse(error.to_string()))?;
    let response_type = envelope
        .get("type")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "sync" | "async" | "error"))
        .ok_or_else(|| {
            SnapdRestError::UnrecognizedResponse("missing recognizable snapd type".to_owned())
        })?
        .to_owned();
    let status_code = envelope
        .get("status-code")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| {
            SnapdRestError::UnrecognizedResponse("missing snapd status-code".to_owned())
        })?;
    let result = envelope
        .get("result")
        .cloned()
        .ok_or_else(|| SnapdRestError::UnrecognizedResponse("missing snapd result".to_owned()))?;
    let change = envelope
        .get("change")
        .and_then(Value::as_str)
        .map(str::to_owned);

    if response_type == "error" {
        let kind = result
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let message = result
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("snapd request failed")
            .to_owned();
        if status_code == 404
            && matches!(
                kind.as_deref(),
                Some("api-not-found" | "endpoint-not-found" | "not-implemented")
            )
        {
            return Err(SnapdRestError::UnsupportedEndpoint {
                path: request_path.to_owned(),
                reason: message,
            });
        }
        return Err(SnapdRestError::Daemon {
            status_code,
            kind,
            message,
            raw_body,
        });
    }

    Ok(SnapdResponse {
        http_status,
        status_code,
        response_type,
        result,
        change,
        raw_body,
    })
}

#[cfg_attr(not(unix), allow(dead_code))]
fn decode_chunked(bytes: &[u8]) -> Result<Vec<u8>, SnapdRestError> {
    let mut remaining = bytes;
    let mut decoded = Vec::new();
    loop {
        let line_end = remaining
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| {
                SnapdRestError::UnrecognizedResponse("invalid chunked body".to_owned())
            })?;
        let size_text = String::from_utf8_lossy(&remaining[..line_end]);
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| SnapdRestError::UnrecognizedResponse("invalid chunk size".to_owned()))?;
        remaining = &remaining[line_end + 2..];
        if size == 0 {
            break;
        }
        if remaining.len() < size + 2 || &remaining[size..size + 2] != b"\r\n" {
            return Err(SnapdRestError::UnrecognizedResponse(
                "truncated chunked body".to_owned(),
            ));
        }
        decoded.extend_from_slice(&remaining[..size]);
        if decoded.len() > MAX_RESPONSE_BYTES {
            return Err(SnapdRestError::UnrecognizedResponse(
                "decoded response exceeded the 8 MiB safety limit".to_owned(),
            ));
        }
        remaining = &remaining[size + 2..];
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::time::Duration;

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(percent_encode("C++ editor"), "C%2B%2B%20editor");
    }

    #[test]
    fn valid_snap_not_found_is_a_daemon_result_not_a_fallback() {
        let response = concat!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\r\n",
            r#"{"type":"error","status-code":404,"status":"Not Found","result":{"message":"snap not found","kind":"snap-not-found","value":"pycharm"}}"#
        );
        let error = parse_http_response(response.as_bytes(), "/v2/find?name=pycharm")
            .expect_err("snap-not-found is an error envelope");
        assert!(matches!(
            error,
            SnapdRestError::Daemon {
                status_code: 404,
                ref kind,
                ..
            } if kind.as_deref() == Some("snap-not-found")
        ));
        assert!(!error.allows_cli_fallback());
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_client_reads_recognizable_response() {
        use std::{fs, os::unix::net::UnixListener, thread};
        let socket =
            std::env::temp_dir().join(format!("allp-snapd-rest-{}.sock", std::process::id()));
        let _ = fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("socket should bind");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("client should connect");
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request).expect("request should be read");
            let body = r#"{"type":"sync","status-code":200,"status":"OK","result":[]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("response should be written");
        });
        let response = SnapdClient::with_timeout(&socket, Duration::from_secs(2))
            .get("/v2/find?q=test&scope=wide")
            .expect("response should parse");
        assert_eq!(response.status_code, 200);
        server.join().expect("server should stop");
        fs::remove_file(socket).expect("socket should be removed");
    }
}
