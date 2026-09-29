//! What this server does with its standard streams.
//!
//! stdout is the JSON-RPC channel and nothing else may write to it. The
//! libraries the tools call print progress with `println!` — `runner down`
//! printed `runner down: port N closed` into the channel, the client failed
//! to parse it, and the server then died on a broken pipe. Rather than
//! chase every print in every library, the channel gets a descriptor of
//! its own and descriptor 1 is pointed at stderr: a stray print lands in
//! the log.

use std::os::fd::{AsFd, AsRawFd};

unsafe extern "C" {
    fn dup2(from: i32, to: i32) -> i32;
}

/// Whether the arguments ask for the version, and if so, print it.
///
/// Answered before anything else touches stdio. Without this, asking the
/// server its version made it treat an empty stdin as a request and print a
/// JSON-RPC parse error — on stdout, in a shape whose digits (`-32700`)
/// read as a version number to anything scraping them. The plugin's
/// readiness hook did exactly that and told sessions there was a version
/// mismatch that did not exist.
pub fn answered_version() -> bool {
    let asked = std::env::args()
        .nth(1)
        .is_some_and(|flag| flag == "--version" || flag == "-V");
    if asked {
        println!("smix-mcp {}", env!("CARGO_PKG_VERSION"));
    }
    asked
}

/// The transport: stdin, and a private copy of stdout that only JSON-RPC
/// writes to. Descriptor 1 goes to stderr from here on.
pub fn protocol() -> std::io::Result<(tokio::io::Stdin, tokio::fs::File)> {
    let channel = std::io::stdout().as_fd().try_clone_to_owned()?;
    // SAFETY: both descriptors are open for the life of the process; dup2
    // only replaces what descriptor 1 refers to.
    if unsafe { dup2(std::io::stderr().as_raw_fd(), 1) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((
        tokio::io::stdin(),
        tokio::fs::File::from_std(std::fs::File::from(channel)),
    ))
}
