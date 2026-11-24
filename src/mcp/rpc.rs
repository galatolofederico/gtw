//! The game's loopback control service.
//!
//! `fableflight-mcp` speaks MCP to the AI client and forwards tool calls here
//! as one JSON-RPC request per line.  The game drains the calls on its main
//! thread, where it can read and change the live simulation safely.

use super::tools;
use crossbeam_channel::{Receiver, Sender, bounded};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Where the bridge looks unless `FABLEFLIGHT_MCP_PORT` or `--port` says otherwise.
pub const DEFAULT_PORT: u16 = 42761;

/// How long a tool may take before the bridge gives up.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest request line we read (the bridge's are a few hundred bytes).
const MAX_LINE: u64 = 1 << 20;

/// One tool call for the game thread.
pub struct Call {
    pub name: String,
    pub args: Value,
    /// The complete MCP `CallToolResult` object.
    pub reply: Sender<Value>,
}

/// Path of the owner-only token file for the loopback MCP server on `port`.
pub fn token_path(port: u16) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fablegame")
        .join(format!("mcp-{port}.token"))
}

/// Reads the MCP auth token from `FABLEFLIGHT_MCP_TOKEN` or the per-port token file.
pub fn read_token(port: u16) -> Option<String> {
    if let Ok(tok) = std::env::var("FABLEFLIGHT_MCP_TOKEN") {
        let tok = tok.trim().to_string();
        if !tok.is_empty() {
            return Some(tok);
        }
    }
    std::fs::read_to_string(token_path(port))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    let mut filled = false;
    #[cfg(unix)]
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        filled = f.read_exact(&mut bytes).is_ok();
    }
    if !filled {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        for chunk in bytes.chunks_mut(8) {
            let mut h = RandomState::new().build_hasher();
            h.write_u32(std::process::id());
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            );
            chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
        }
    }
    let mut out = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn write_token_file(port: u16, token: &str) -> std::io::Result<()> {
    let path = token_path(port);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("token.tmp.{}", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(token.as_bytes())?;
    f.write_all(b"\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path)
}

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Start listening on `127.0.0.1:port` and return the queue the game drains.
/// Writes a per-session secret token to `token_path(port)` (mode 0600 on Unix)
/// so only the local user's `fableflight-mcp` bridge can issue commands.
pub fn serve(port: u16) -> std::io::Result<Receiver<Call>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let bound_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    let token = std::env::var("FABLEFLIGHT_MCP_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(generate_token);
    if let Err(e) = write_token_file(bound_port, &token) {
        log::warn!("could not write MCP token file: {e}");
    }
    serve_listener(listener, Some(token))
}

fn serve_listener(listener: TcpListener, token: Option<String>) -> std::io::Result<Receiver<Call>> {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = token.map(Arc::<str>::from);
    let (tx, rx) = bounded(256);
    std::thread::Builder::new().name("mcp-rpc".into()).spawn(move || {
        log::info!("MCP copilot listening on 127.0.0.1:{port}");
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let tx = tx.clone();
                    let token = token.clone();
                    let _ = std::thread::Builder::new().name("mcp-call".into()).spawn(move || handle(stream, tx, token.as_deref()));
                }
                Err(e) => log::warn!("MCP connection: {e}"),
            }
        }
    })?;
    Ok(rx)
}

fn handle(stream: TcpStream, tx: Sender<Call>, expected_token: Option<&str>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
    let Ok(reader_stream) = stream.try_clone() else { return };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let mut line = String::new();
    loop {
        line.clear();
        match (&mut reader).take(MAX_LINE).read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        // (a line that long is not one of the bridge's)
        if line.len() as u64 >= MAX_LINE && !line.ends_with('\n') {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // The bridge only ever sends JSON objects. Anything else is another
        // program on this port (a web page's HTTP request, say): hang up
        // before reading whatever it sends next.
        if !line.starts_with('{') {
            break;
        }
        let (close, Some(response)) = response(line, &tx, expected_token) else { continue };
        let mut text = match serde_json::to_string(&response) {
            Ok(text) => text,
            Err(e) => {
                log::warn!("MCP response: {e}");
                if close {
                    break;
                }
                continue;
            }
        };
        text.push('\n');
        if writer.write_all(text.as_bytes()).is_err() || writer.flush().is_err() || close {
            break;
        }
    }
}

fn response(line: &str, tx: &Sender<Call>, expected_token: Option<&str>) -> (bool, Option<Value>) {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return (false, Some(error(Value::Null, -32700, format!("parse error: {e}")))),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if let Some(want) = expected_token {
        let got = request.get("token").and_then(Value::as_str).unwrap_or("");
        if !ct_eq(got, want) {
            return (true, Some(error(id, -32001, "unauthorized: missing or invalid MCP token".into())));
        }
    }
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if method.is_empty() || method.starts_with("notifications/") {
        return (false, None);
    }
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let reply = match method {
        "ping" => Some(ok(id, json!({}))),
        "tools/list" => Some(ok(id, tools::catalog_json())),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            if name.is_empty() {
                return (false, Some(error(id, -32602, "tools/call needs a tool name".into())));
            }
            let mut args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if args.is_null() {
                args = json!({});
            }
            let (reply, answer) = bounded(1);
            if tx.send_timeout(Call { name, args, reply }, Duration::from_secs(2)).is_err() {
                return (false, Some(ok(id, tools::error_result("The simulator is busy; try again in a moment."))));
            }
            match answer.recv_timeout(CALL_TIMEOUT) {
                Ok(result) => Some(ok(id, result)),
                Err(_) => Some(error(id, -32603, "the simulator did not answer; is it still running?".into())),
            }
        }
        other => Some(error(id, -32601, format!("unknown method {other}"))),
    };
    (false, reply)
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bridge's side of the loopback protocol: a call with the right token
    /// reaches the game queue and the game's result reaches the socket.
    #[test]
    fn tool_call_round_trip() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = serve_listener(listener, Some("test-secret-token".into())).unwrap();
        let game = std::thread::spawn(move || {
            let call = calls.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(call.name, "get_aircraft_state");
            assert_eq!(call.args, json!({ "limit": 1 }));
            call.reply.send(tools::json_result(json!({ "ok": true }))).unwrap();
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"token\":\"test-secret-token\",\"method\":\"tools/call\",\"params\":{\"name\":\"get_aircraft_state\",\"arguments\":{\"limit\":1}}}\n").unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(answer["id"], 7);
        assert_eq!(answer["result"]["isError"], false);
        assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains("\"ok\""));
        game.join().unwrap();
    }

    /// A local process without the per-session token is rejected and never
    /// reaches the game queue.
    #[test]
    fn missing_or_wrong_token_is_rejected() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = serve_listener(listener, Some("expected-token".into())).unwrap();
        for payload in [
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"set_gear\",\"arguments\":{}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"token\":\"wrong\",\"method\":\"tools/call\",\"params\":{\"name\":\"set_gear\",\"arguments\":{}}}\n",
        ] {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            stream.write_all(payload.as_bytes()).unwrap();
            stream.flush().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let answer: Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(answer["error"]["code"], -32001);
        }
        assert!(calls.recv_timeout(Duration::from_millis(200)).is_err());
    }

    /// A web page can make a browser send an HTTP request here whose body is
    /// a tool call. The request line is not JSON: the connection closes and
    /// the body is never read.
    #[test]
    fn http_request_is_hung_up_on() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = serve_listener(listener, Some("expected-token".into())).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _ = stream.write_all(b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"set_gear\",\"arguments\":{}}}\n");
        let _ = stream.flush();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
        assert!(answer.is_empty(), "answered an HTTP request: {}", String::from_utf8_lossy(&answer));
        assert!(calls.recv_timeout(Duration::from_millis(300)).is_err(), "the tool call in the HTTP body reached the game");
    }
}
