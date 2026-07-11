use codux_remote_transport::WebTunnelTcpConnectRequest;
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use url::Url;

const SESSION_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_HEADER_BYTES: usize = 64 * 1024;
const DEFAULT_WEB_TUNNEL_URL: &str = "http://127.0.0.1:8765/";

#[derive(Clone)]
pub struct HostBrowserProxy {
    shared: Arc<ProxyShared>,
}

struct ProxyShared {
    sessions: Mutex<HashMap<String, ProxySession>>,
}

#[derive(Clone)]
struct ProxySession {
    device_id: String,
    device_token: String,
    controller: Arc<dyn HostBrowserController>,
    expires_at: Instant,
}

pub trait HostBrowserController: Send + Sync {
    fn tcp_connect(
        &self,
        request: WebTunnelTcpConnectRequest,
    ) -> Result<Box<dyn codux_remote_transport::WebTunnelIoStream>, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostBrowserOpenResult {
    pub original_url: String,
    pub proxy_host: String,
    pub proxy_port: u16,
}

impl HostBrowserProxy {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(ProxyShared {
                sessions: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn open(
        &self,
        device_id: String,
        device_token: String,
        target_url: &str,
        controller: Arc<dyn HostBrowserController>,
    ) -> Result<HostBrowserOpenResult, String> {
        let target = validate_web_url(target_url)?;
        let token = uuid::Uuid::new_v4().to_string();
        let listener = crate::async_runtime::block_on(TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|error| error.to_string())?;
        self.register_session(
            token.clone(),
            ProxySession {
                device_id,
                device_token,
                controller,
                expires_at: Instant::now() + SESSION_TTL,
            },
        )?;
        let port = spawn_tunnel_proxy_listener(listener, Arc::clone(&self.shared), token)?;
        Ok(HostBrowserOpenResult {
            original_url: target.to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port,
        })
    }

    pub fn open_session(
        &self,
        device_id: String,
        device_token: String,
        controller: Arc<dyn HostBrowserController>,
    ) -> Result<HostBrowserOpenResult, String> {
        let token = uuid::Uuid::new_v4().to_string();
        let listener = crate::async_runtime::block_on(TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|error| error.to_string())?;
        self.register_session(
            token.clone(),
            ProxySession {
                device_id,
                device_token,
                controller,
                expires_at: Instant::now() + SESSION_TTL,
            },
        )?;
        let port = spawn_tunnel_proxy_listener(listener, Arc::clone(&self.shared), token)?;
        Ok(HostBrowserOpenResult {
            original_url: DEFAULT_WEB_TUNNEL_URL.to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port,
        })
    }

    fn register_session(&self, token: String, session: ProxySession) -> Result<(), String> {
        let mut sessions = self
            .shared
            .sessions
            .lock()
            .map_err(|_| "web tunnel session lock poisoned".to_string())?;
        sessions.retain(|_, session| session.expires_at > Instant::now());
        sessions.insert(token, session);
        Ok(())
    }
}

impl Default for HostBrowserProxy {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_tunnel_proxy_listener(
    listener: TcpListener,
    shared: Arc<ProxyShared>,
    default_token: String,
) -> Result<u16, String> {
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();
    crate::async_runtime::spawn(async move {
        let _ = ready_tx.send(());
        serve_tunnel_proxy(listener, shared, default_token).await;
    });
    let _ = crate::async_runtime::block_on(ready_rx);
    Ok(port)
}

async fn serve_tunnel_proxy(
    listener: TcpListener,
    shared: Arc<ProxyShared>,
    default_token: String,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let shared = Arc::clone(&shared);
        let default_token = default_token.clone();
        crate::async_runtime::spawn(async move {
            let _ = handle_proxy_stream(stream, shared, default_token).await;
        });
    }
}

async fn handle_proxy_stream(
    stream: TcpStream,
    shared: Arc<ProxyShared>,
    default_token: String,
) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let request = match read_proxy_head(&mut reader).await {
        Ok(request) => request,
        Err(error) => {
            let mut stream = reader.into_inner();
            let _ = write_error_response(&mut stream, 400, &error.to_string()).await;
            return Ok(());
        }
    };
    if request.method.eq_ignore_ascii_case("CONNECT") {
        return handle_connect_request(reader, request, shared, default_token).await;
    }
    handle_absolute_http_request(reader, request, shared, default_token).await
}

async fn handle_connect_request(
    reader: BufReader<TcpStream>,
    request: ProxyHead,
    shared: Arc<ProxyShared>,
    default_token: String,
) -> io::Result<()> {
    let (host, port) = match parse_connect_authority(&request.target) {
        Ok(target) => target,
        Err(error) => {
            let mut client = reader.into_inner();
            return write_error_response(&mut client, 400, &error).await;
        }
    };
    tunnel_to_host(reader, shared, &default_token, host, port, None).await
}

async fn handle_absolute_http_request(
    reader: BufReader<TcpStream>,
    request: ProxyHead,
    shared: Arc<ProxyShared>,
    default_token: String,
) -> io::Result<()> {
    let target_url = match validate_web_url(&request.target) {
        Ok(url) => url,
        Err(error) => {
            let mut client = reader.into_inner();
            return write_error_response(&mut client, 400, &error).await;
        }
    };
    let host = match target_url.host_str() {
        Some(host) => host.to_string(),
        None => {
            let mut client = reader.into_inner();
            return write_error_response(&mut client, 400, "missing target host").await;
        }
    };
    let port = target_url.port_or_known_default().unwrap_or(80);
    tunnel_to_host(
        reader,
        shared,
        &default_token,
        host,
        port,
        Some((request, target_url)),
    )
    .await
}

async fn tunnel_to_host(
    reader: BufReader<TcpStream>,
    shared: Arc<ProxyShared>,
    token: &str,
    host: String,
    port: u16,
    http_request: Option<(ProxyHead, Url)>,
) -> io::Result<()> {
    let Some(session) = session_for_token(&shared, token) else {
        let mut client = reader.into_inner();
        return write_error_response(&mut client, 410, "web tunnel session expired").await;
    };
    if is_forbidden_host(&host) {
        let mut client = reader.into_inner();
        return write_error_response(&mut client, 403, "target is not allowed").await;
    }
    let mut remote = match session.controller.tcp_connect(WebTunnelTcpConnectRequest {
        device_id: session.device_id,
        device_token: session.device_token,
        host,
        port,
    }) {
        Ok(stream) => stream,
        Err(error) => {
            let mut client = reader.into_inner();
            return write_error_response(&mut client, 502, &error).await;
        }
    };
    let buffered = reader.buffer().to_vec();
    let mut client = reader.into_inner();
    if let Some((request, target_url)) = http_request {
        write_origin_form_request(&mut remote, &request, &target_url).await?;
    } else {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
    }
    if !buffered.is_empty() {
        remote.write_all(&buffered).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
    Ok(())
}

async fn write_origin_form_request<W>(
    writer: &mut W,
    request: &ProxyHead,
    target_url: &Url,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let path_and_query = target_url[url::Position::BeforePath..].to_string();
    writer
        .write_all(
            format!(
                "{} {} {}\r\n",
                request.method, path_and_query, request.version
            )
            .as_bytes(),
        )
        .await?;
    let mut has_host = false;
    for (name, value) in &request.headers {
        if name.eq_ignore_ascii_case("proxy-connection") {
            continue;
        }
        if name.eq_ignore_ascii_case("host") {
            has_host = true;
        }
        writer
            .write_all(format!("{name}: {value}\r\n").as_bytes())
            .await?;
    }
    if !has_host && let Some(host) = target_url.host_str() {
        let host_header = if let Some(port) = target_url.port() {
            format!("{host}:{port}")
        } else {
            host.to_string()
        };
        writer
            .write_all(format!("Host: {host_header}\r\n").as_bytes())
            .await?;
    }
    writer.write_all(b"\r\n").await
}

#[derive(Debug)]
struct ProxyHead {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
}

async fn read_proxy_head<R>(reader: &mut R) -> io::Result<ProxyHead>
where
    R: AsyncBufRead + Unpin,
{
    let mut total = 0usize;
    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line).await?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "empty request",
        ));
    }
    total += n;
    if total > MAX_HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "headers too large",
        ));
    }
    let mut parts = request_line
        .trim_end_matches(['\r', '\n'])
        .split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    if method.is_empty() || target.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request line",
        ));
    }
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        total += n;
        if total > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    Ok(ProxyHead {
        method,
        target,
        version,
        headers,
    })
}

async fn write_error_response<W>(writer: &mut W, status: u16, message: &str) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let body = message.as_bytes();
    writer
        .write_all(
            format!(
                "HTTP/1.1 {status} Error\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    writer.write_all(body).await
}

fn session_for_token(shared: &ProxyShared, token: &str) -> Option<ProxySession> {
    let mut sessions = shared.sessions.lock().ok()?;
    sessions.retain(|_, session| session.expires_at > Instant::now());
    sessions.get(token).cloned()
}

fn validate_web_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| format!("invalid URL: {error}"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err("only http and https URLs are supported".to_string()),
    }
}

fn parse_connect_authority(value: &str) -> Result<(String, u16), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("missing CONNECT target".to_string());
    }
    if let Some(stripped) = value.strip_prefix('[') {
        let Some((host, rest)) = stripped.split_once(']') else {
            return Err("invalid CONNECT IPv6 target".to_string());
        };
        let port = rest
            .strip_prefix(':')
            .ok_or_else(|| "missing CONNECT port".to_string())?
            .parse::<u16>()
            .map_err(|_| "invalid CONNECT port".to_string())?;
        return Ok((host.to_string(), port));
    }
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| "missing CONNECT port".to_string())?;
    let port = port
        .parse::<u16>()
        .map_err(|_| "invalid CONNECT port".to_string())?;
    Ok((host.to_string(), port))
}

fn is_forbidden_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    if let Ok(ip) = IpAddr::from_str(host) {
        return is_forbidden_ip(ip);
    }
    false
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            octets == [169, 254, 169, 254]
        }
        IpAddr::V6(ip) => ip.is_unspecified(),
    }
}

pub fn is_web_url(value: &str) -> bool {
    validate_web_url(value).is_ok()
}

pub fn socket_addr_label(addr: SocketAddr) -> String {
    addr.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_url_rejects_non_web_schemes() {
        assert!(is_web_url("http://localhost:5173"));
        assert!(!is_web_url("file:///etc/passwd"));
    }

    #[test]
    fn connect_authority_parses_domain_and_ipv6() {
        assert_eq!(
            parse_connect_authority("localhost:5173").unwrap(),
            ("localhost".to_string(), 5173)
        );
        assert_eq!(
            parse_connect_authority("[::1]:3000").unwrap(),
            ("::1".to_string(), 3000)
        );
    }
}
