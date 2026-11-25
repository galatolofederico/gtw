//! The MCP bridge on its own: no GPU, no game, just the stdio protocol.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Bridge {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Bridge {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_gtw-mcp"))
            // A port nothing is listening on: tool calls must answer with a
            // useful tool-level error instead of hanging.
            .env("GTW_MCP_PORT", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start gtw-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, stdin, lines }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").expect("write MCP message");
        self.stdin.flush().expect("flush MCP message");
    }

    fn receive(&mut self) -> Value {
        let line = self.lines.recv_timeout(Duration::from_secs(10)).expect("a response from the bridge");
        serde_json::from_str(&line).expect("a JSON-RPC response")
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn handshake_tools_and_offline_error() {
    let mut bridge = Bridge::start();
    bridge.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" }
        }
    }));
    let init = bridge.receive();
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert!(init["result"]["instructions"].as_str().unwrap().contains("copilot"));
    assert_eq!(init["result"]["serverInfo"]["name"], "gtw");

    bridge.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    bridge.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = bridge.receive();
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for wanted in [
        "get_aircraft_state",
        "get_navigation",
        "get_environment",
        "get_airport_info",
        "set_aircraft_config",
        "set_engine",
        "set_radios",
        "set_autopilot",
        "set_environment",
        "place_aircraft",
        "take_screenshot",
    ] {
        assert!(names.contains(&wanted), "missing {wanted} in {names:?}");
    }
    assert!(tools["result"]["tools"].as_array().unwrap().iter().all(|t| t["inputSchema"]["type"] == "object"));

    bridge.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }));
    assert_eq!(bridge.receive()["result"], json!({}));

    bridge.send(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "get_aircraft_state", "arguments": {} } }));
    let call = bridge.receive();
    assert_eq!(call["result"]["isError"], true, "no game is running on port 1");
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("not running"));
}
