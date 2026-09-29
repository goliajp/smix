//! A library's `println!` must not reach the JSON-RPC channel.
//!
//! `smix_release` calls `runner down`, which prints `runner down: port N
//! closed`. That line used to arrive on stdout between two JSON-RPC
//! messages; the client could not parse it and the server died on a broken
//! pipe. The release here runs against a port nothing listens on, with a
//! made-up UDID and a ledger of its own, so no device and no real record
//! is touched.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
fn runner_downs_line_goes_to_stderr_and_the_next_stdout_line_is_the_answer() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port();
    let machine = std::env::temp_dir().join(format!("smix-mcp-stray-print-{}", std::process::id()));
    std::fs::create_dir_all(&machine).expect("machine dir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_smix-mcp"))
        .env("SMIX_RUNNER_PORT", port.to_string())
        .env("SMIX_UDID", "00000000-0000-4000-8000-000000000000")
        .env("SMIX_MACHINE_DIR", &machine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("smix-mcp spawns");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut next = || {
        let mut line = String::new();
        stdout.read_line(&mut line).expect("read");
        serde_json::from_str::<serde_json::Value>(&line)
            .unwrap_or_else(|e| panic!("not JSON-RPC on stdout: {line:?} ({e})"))
    };

    let init = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}});
    writeln!(stdin, "{init}").expect("write");
    assert_eq!(next()["id"], 1);
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .expect("write");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"smix_release","arguments":{{}}}}}}"#
    )
    .expect("write");
    let answer = next();
    assert_eq!(answer["id"], 2, "{answer}");

    drop(stdin);
    let _ = child.wait();
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut err)
        .expect("stderr");
    let _ = std::fs::remove_dir_all(&machine);
    // The print still happens; it is the path this test exists for.
    assert!(
        err.contains("runner down"),
        "the release never reached `runner down`: {err}"
    );
}
