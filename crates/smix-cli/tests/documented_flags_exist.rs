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
//! This reads the guides, pulls out every `smix …` command they print,
//! and asks the binary itself: does that flag exist, and does that
//! subcommand accept a positional argument at all? `smix runner down
//! <udid>` shipped in the quickstart teardown step and fails with
//! "unexpected argument" — a flags-only check waves it through, which
//! is why this also counts positionals.

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
#[allow(clippy::type_complexity)]
fn commands_in(doc: &str) -> Vec<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut out = Vec::new();
    for line in doc.lines() {
        let line = line.trim().trim_start_matches("$ ");
        let Some(rest) = line.strip_prefix("smix ") else {
            continue;
        };
        // Everything past a pipe / redirect / `&&` belongs to another
        // program, and a trailing `#` starts a comment. Counting those
        // as positionals reads `smix tree --json | jq .` as a four-arg
        // invocation.
        let rest = rest
            .split(" | ")
            .next()
            .unwrap_or(rest)
            .split(" # ")
            .next()
            .unwrap_or(rest)
            .split(" && ")
            .next()
            .unwrap_or(rest)
            .split(" || ")
            .next()
            .unwrap_or(rest)
            .split(" > ")
            .next()
            .unwrap_or(rest);
        let mut path: Vec<String> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        let mut extras: Vec<String> = Vec::new();
        let mut prev_was_flag = false;
        for tok in rest.split_whitespace() {
            if let Some(f) = tok.strip_prefix("--") {
                // `--flag=value` and trailing punctuation from prose.
                let f = f.split('=').next().unwrap_or(f);
                let f = f.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '-');
                if !f.is_empty() {
                    flags.push(f.to_string());
                }
                prev_was_flag = true;
                continue;
            } else if flags.is_empty()
                && extras.is_empty()
                && tok.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !tok.is_empty()
            {
                path.push(tok.to_string());
            } else if prev_was_flag {
                // A flag's value, not a positional.
            } else if !tok.starts_with('\\') && !tok.starts_with('#') && !tok.starts_with('|') {
                extras.push(tok.to_string());
            }
            prev_was_flag = false;
        }
        if !path.is_empty() && (!flags.is_empty() || !extras.is_empty()) {
            out.push((path, flags, extras));
        }
    }
    out
}

/// How many positional arguments does this subcommand take?
fn positional_arity(help: &str) -> usize {
    // clap prints `Usage: smix runner down [OPTIONS]` — positionals show
    // as `<NAME>` or `[NAME]` in that line.
    help.lines()
        .find(|l| l.trim_start().starts_with("Usage:"))
        .map(|usage| {
            usage
                .split_whitespace()
                .filter(|t| {
                    (t.starts_with('<') || t.starts_with('['))
                        && !t.contains("OPTIONS")
                        && !t.contains("COMMAND")
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn every_command_the_guides_print_matches_the_cli_surface() {
    let mut checked = 0usize;
    let mut bogus: Vec<String> = Vec::new();

    for (name, doc) in GUIDES {
        for (path, flags, extras) in commands_in(doc) {
            let refs: Vec<&str> = path.iter().map(String::as_str).collect();
            // A path the binary does not know is prose that happened to
            // parse like a command, or a placeholder; skip rather than
            // guess.
            let Some(help) = help_for(&refs) else {
                continue;
            };
            for flag in &flags {
                if help_declares_flag(&help, flag) {
                    checked += 1;
                } else {
                    bogus.push(format!("{name}: `smix {} --{flag}`", refs.join(" ")));
                }
            }
            // Positional arity: the doc line's leftover tokens after the
            // subcommand path, minus flag values, must fit.
            let arity = positional_arity(&help);
            if arity == 0 && !extras.is_empty() {
                bogus.push(format!(
                    "{name}: `smix {} {}` — that subcommand takes no positional argument",
                    refs.join(" "),
                    extras.join(" ")
                ));
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

/// Repo-relative paths the guides tell a reader to open or run must
/// exist. `flows/00_smoke.yaml` sat in the quickstart's "run a flow"
/// step for its whole life; the repo's example has always been
/// `examples/hello.yaml`, so the one command the page builds up to
/// pointed at nothing.
#[test]
fn every_repo_path_the_guides_name_exists() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    // Directories that exist in the repo — a token starting with one of
    // these is a path claim, not prose.
    const ROOTS: &[&str] = &[
        "examples/",
        "flows/",
        "docs/",
        "crates/",
        "scripts/",
        "swift-bridge/",
        "android-runner/",
        "npm/",
        "web/",
        "dashboard/",
    ];

    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for (name, doc) in GUIDES {
        for raw in doc.split(|c: char| c.is_whitespace() || c == '`' || c == '(' || c == ')') {
            let tok = raw.trim_matches(|c: char| matches!(c, ',' | '.' | ':' | ';' | '"' | '\''));
            if tok.contains('<') || tok.contains('*') || tok.contains('$') {
                continue; // placeholder, not a path
            }
            if !ROOTS.iter().any(|r| tok.starts_with(r)) {
                continue;
            }
            // Only claims that name a file (have an extension).
            if !tok.rsplit('/').next().is_some_and(|f| f.contains('.')) {
                continue;
            }
            checked += 1;
            if !root.join(tok).exists() {
                missing.push(format!("{name}: {tok}"));
            }
        }
    }

    assert!(
        checked >= 3,
        "found only {checked} repo paths in the guides — the extraction \
         stopped matching and this check would pass by knowing nothing. \
         (The guides cite few in-repo files; 3 is the floor observed \
         when this was written, not a target.)"
    );
    assert!(
        missing.is_empty(),
        "the guides name repo files that do not exist:\n  {}",
        missing.join("\n  ")
    );
}
