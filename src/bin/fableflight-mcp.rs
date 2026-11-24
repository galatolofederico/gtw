//! `fableflight-mcp`: the MCP server an AI client launches to copilot a
//! running FableFlight.  It speaks MCP over stdin/stdout and forwards tool
//! calls to the game's loopback control service.

use fablegame::mcp::{rpc, tools};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// A remote client has plenty of time; the game answers within a frame.
const GAME_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let port = port();
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("fableflight-mcp: reading stdin: {e}");
                break;
            }
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(response) = handle(line, port) {
            match serde_json::to_string(&response) {
                Ok(text) => {
                    if output.write_all(text.as_bytes()).is_err() || output.write_all(b"\n").is_err() || output.flush().is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("fableflight-mcp: encoding a response: {e}"),
            }
        }
    }
}

fn port() -> u16 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                if let Some(value) = args.next().and_then(|v| v.parse::<u16>().ok()) {
                    return value;
                }
            }
            other => {
                if let Some(value) = other.strip_prefix("--port=").and_then(|v| v.parse::<u16>().ok()) {
                    return value;
                }
                if other == "--help" || other == "-h" {
                    eprintln!(
                        "fableflight-mcp: MCP stdio bridge for FableFlight\n\
                         usage: fableflight-mcp [--port PORT]\n\
                         The running game listens on 127.0.0.1:42761 by default.\n\
                         FABLEFLIGHT_MCP_PORT sets the port too."
                    );
                }
            }
        }
    }
    std::env::var("FABLEFLIGHT_MCP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(rpc::DEFAULT_PORT)
}

fn handle(line: &str, port: u16) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, format!("parse error: {e}"))),
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if method.is_empty() || method.starts_with("notifications/") {
        return None;
    }
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => Some(ok(id, initialize(&params))),
        "ping" => Some(ok(id, json!({}))),
        "tools/list" => Some(ok(id, tools::catalog_json())),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            if name.is_empty() {
                return Some(error(id, -32602, "tools/call needs a tool name".into()));
            }
            let mut args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if args.is_null() {
                args = json!({});
            }
            let result = match call_game(port, name, &args) {
                Ok(result) => result,
                Err(e) => tools::error_result(e),
            };
            Some(ok(id, result))
        }
        other => Some(error(id, -32601, format!("unknown method {other}"))),
    }
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-11-25");
    // The bridge implements the tools-only legacy handshake.  Echoing a
    // version the client asked for keeps old clients happy; anything modern
    // falls back to the last version this server knows.
    let supported = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
    let protocol = if supported.contains(&requested) { requested } else { "2025-11-25" };
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "fableflight", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "You are the copilot of a running FableFlight flight simulator. \
            Read get_aircraft_state before acting. Units: feet, knots, ft/min, degrees true, hPa, kg. \
            You may set the throttle, aircraft configuration, engine, radios, weather, time and the aircraft's position. \
            You cannot move the stick; the human pilot keeps attitude. \
            You may engage and set the autopilot (set_autopilot) when the pilot asks; any firm input of theirs disconnects it. \
            Do not change weather, time or position unless the pilot asks. \
            Use get_navigation and get_airport_info for approach and radio work. \
            take_screenshot returns an image when seeing the panel or the view helps."
    })
}

/// Forward one tool call to the running game.
fn call_game(port: u16, name: &str, args: &Value) -> Result<Value, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| {
        format!(
            "FableFlight is not running (could not reach 127.0.0.1:{port}: {e}). \
             Start the simulator (the MCP service is on by default; --no-mcp disables it)."
        )
    })?;
    stream.set_read_timeout(Some(GAME_TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(GAME_TIMEOUT)).map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    if let Some(token) = rpc::read_token(port) {
        request["token"] = Value::String(token);
    }
    let mut text = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    text.push('\n');
    writer.write_all(text.as_bytes()).map_err(|e| format!("writing to FableFlight: {e}"))?;
    writer.flush().map_err(|e| format!("writing to FableFlight: {e}"))?;
    let mut reply = String::new();
    reader.read_line(&mut reply).map_err(|e| format!("reading from FableFlight: {e}"))?;
    if reply.trim().is_empty() {
        return Err("FableFlight closed the connection without answering.".into());
    }
    let value: Value = serde_json::from_str(reply.trim()).map_err(|e| format!("bad answer from FableFlight: {e}"))?;
    if let Some(result) = value.get("result") {
        return Ok(result.clone());
    }
    if let Some(error) = value.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
        return Err(format!("FableFlight: {message}"));
    }
    Err("FableFlight sent a response with neither result nor error.".into())
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
