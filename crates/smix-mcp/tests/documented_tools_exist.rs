//! Every MCP call the docs print must be a call the server accepts.
//!
//! fact-scan checks that "N tools" equals the number registered — a
//! count, which stays true while every name under it drifts. The names
//! and argument keys are what an agent actually sends, and nothing
//! compared them to the schemas the server advertises.
//!
//! This asks the server itself, over the same stdio transport an MCP
//! client uses: spawn it, handshake, list tools, and check each
//! documented example against the advertised inputSchema. It needs no
//! runner — the server connects lazily, so `tools/list` answers with
//! nothing booted.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MCP_GUIDE: &str = include_str!("../../../docs/ai-guide/11-mcp.md");
const MCP_README: &str = include_str!("../README.md");

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_smix-mcp"))
            .env("SMIX_RUNNER_PORT", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("smix-mcp spawns");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Server {
            child,
            stdin,
            stdout,
        }
    }

    fn call(&mut self, req: serde_json::Value) -> serde_json::Value {
        writeln!(self.stdin, "{req}").expect("write");
        self.stdin.flush().expect("flush");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {line:?} ({e})"))
    }

    fn notify(&mut self, req: serde_json::Value) {
        writeln!(self.stdin, "{req}").expect("write");
        self.stdin.flush().expect("flush");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `smix_tap { "id": "x", "text": "y" }` → ("smix_tap", ["id", "text"]).
/// Only depth-1 keys count: `smix_fill { "target": { "id": … } }` sends
/// `target`, and `id` belongs to the selector nested inside it.
fn documented_calls(doc: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let bytes: Vec<char> = doc.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if !doc[byte_index(&bytes, i)..].starts_with("smix_") {
            i += 1;
            continue;
        }
        let start = byte_index(&bytes, i);
        let name: String = doc[start..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || *c == '_')
            .collect();
        let after = &doc[start + name.len()..];
        // The call's argument object must follow on the same line, or
        // this is prose mentioning the tool rather than showing a call.
        let head: String = after.chars().take_while(|c| *c != '\n').collect();
        if !head.trim_start().starts_with('{') {
            i += name.chars().count();
            continue;
        }
        let mut depth = 0usize;
        let mut keys = Vec::new();
        let mut chars = after.char_indices().peekable();
        while let Some((idx, c)) = chars.next() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                '"' if depth == 1 => {
                    let rest = &after[idx + 1..];
                    let key: String = rest.chars().take_while(|c| *c != '"').collect();
                    let post = &rest[key.len()..];
                    if post.starts_with("\":") || post.starts_with("\" :") {
                        keys.push(key.clone());
                    }
                    // Skip past the closing quote so a value is not read
                    // as a key.
                    for _ in 0..key.chars().count() + 1 {
                        chars.next();
                    }
                }
                _ => {}
            }
        }
        out.push((name.clone(), keys));
        i += name.chars().count();
    }
    out
}

fn byte_index(chars: &[char], i: usize) -> usize {
    chars[..i].iter().map(|c| c.len_utf8()).sum()
}

#[test]
fn every_documented_mcp_call_matches_the_advertised_schema() {
    let mut server = Server::start();
    let init = server.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "doc-gate", "version": "0" }
        }
    }));
    assert!(
        init.get("result").is_some(),
        "the server must complete a handshake with no runner up — an MCP \
         client starts it long before anyone runs `smix runner up`: {init}"
    );
    server.notify(serde_json::json!({
        "jsonrpc": "2.0", "method": "notifications/initialized"
    }));

    let listed = server.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/list"
    }));
    let tools = listed["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list returned {listed}"));

    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (source, doc) in [("11-mcp", MCP_GUIDE), ("smix-mcp/README", MCP_README)] {
        for (name, keys) in documented_calls(doc) {
            let Some(tool) = tools.iter().find(|t| t["name"] == name.as_str()) else {
                problems.push(format!("{source}: no such tool `{name}`"));
                continue;
            };
            let props = tool["inputSchema"]["properties"]
                .as_object()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for key in keys {
                checked += 1;
                if !props.contains(&key) {
                    problems.push(format!(
                        "{source}: `{name}` has no argument `{key}` (accepts {props:?})"
                    ));
                }
            }
        }
    }

    assert!(
        checked >= 5,
        "extracted only {checked} argument keys from the MCP docs — the \
         extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    assert!(
        problems.is_empty(),
        "the MCP docs show calls the server would reject:\n  {}",
        problems.join("\n  ")
    );
}
