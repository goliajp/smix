//! Every flag the guides tell a user to type must be a flag the CLI has.
//!
//! `smix runner up --soft --no-capture` was in the quickstart, the
//! cookbook, the MCP guide, the CLI reference and a run-script example.
//! Those two flags belong to `capsule up`; `runner up` has never
//! accepted them, so the first command the quickstart asks a new user to
//! run failed on the flag parse. Nothing connected the prose to the
//! parser, so five documents agreed with each other and none with the
//! code.
//!
//! This reads the guides, pulls out every `smix <sub> --flag` they
//! print, and asks clap whether it exists.

use std::process::Command;

/// The guides that show commands a reader is meant to type.
const GUIDES: &[(&str, &str)] = &[
    (
        "01-quickstart",
        include_str!("../../../docs/ai-guide/01-quickstart.md"),
    ),
    ("05-cli", include_str!("../../../docs/ai-guide/05-cli.md")),
    (
        "08-cookbook",
        include_str!("../../../docs/ai-guide/08-cookbook.md"),
    ),
    ("11-mcp", include_str!("../../../docs/ai-guide/11-mcp.md")),
];

/// `smix <path...> --help`, or None when that path is not a command.
/// The binary's own help IS the surface a reader is copying from, so
/// asking it is asking the same source they would.
fn help_for(path: &[&str]) -> Option<String> {
    let exe = env!("CARGO_BIN_EXE_smix");
    let out = Command::new(exe).args(path).arg("--help").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn help_declares_flag(help: &str, flag: &str) -> bool {
    help.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with(&format!("--{flag}"))
            || l.contains(&format!(", --{flag} "))
            || l.contains(&format!(", --{flag}<"))
            || l.trim_end() == format!("--{flag}")
    })
}

/// `smix runner up --supervise` → (["runner","up"], "supervise").
/// Only lines that start a command are considered; continuation lines
/// and prose are ignored, which is why the flag must be preceded by a
/// recognized subcommand path on the same line.
fn commands_in(doc: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let mut out = Vec::new();
    for line in doc.lines() {
        let line = line.trim().trim_start_matches("$ ");
        let Some(rest) = line.strip_prefix("smix ") else {
            continue;
        };
        let mut path: Vec<String> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        for tok in rest.split_whitespace() {
            if let Some(f) = tok.strip_prefix("--") {
                // `--flag=value` and trailing punctuation from prose.
                let f = f.split('=').next().unwrap_or(f);
                let f = f.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '-');
                if !f.is_empty() {
                    flags.push(f.to_string());
                }
            } else if flags.is_empty()
                && tok.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !tok.is_empty()
            {
                path.push(tok.to_string());
            }
        }
        if !path.is_empty() && !flags.is_empty() {
            out.push((path, flags));
        }
    }
    out
}

#[test]
fn every_flag_the_guides_print_is_a_flag_the_cli_accepts() {
    let mut checked = 0usize;
    let mut bogus: Vec<String> = Vec::new();

    for (name, doc) in GUIDES {
        for (path, flags) in commands_in(doc) {
            let refs: Vec<&str> = path.iter().map(String::as_str).collect();
            // A path the binary does not know is prose that happened to
            // parse like a command, or a placeholder; skip rather than
            // guess.
            let Some(help) = help_for(&refs) else {
                continue;
            };
            for flag in flags {
                if help_declares_flag(&help, &flag) {
                    checked += 1;
                } else {
                    bogus.push(format!("{name}: `smix {} --{flag}`", refs.join(" ")));
                }
            }
        }
    }

    assert!(
        checked >= 10,
        "extracted only {checked} flag usages from the guides — the \
         extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    assert!(
        bogus.is_empty(),
        "the guides tell users to type flags the CLI does not have:\n  {}",
        bogus.join("\n  ")
    );
}
