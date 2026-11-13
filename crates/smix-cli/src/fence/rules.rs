//! Which shell commands start, stop or create a device outside smix.
//!
//! Judged by what a command runs, not by what its text mentions: a `grep`
//! for `simctl boot` reads about the rule and is not refused. Pure, so
//! every rule is a row in a test table.

/// What the fence makes of one Bash call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Nothing here changes a device's life.
    Allow,
    /// A lifecycle step taken outside smix; the text says what to use.
    Deny(String),
    /// Lifecycle steps let through by a named exception, each with its
    /// reason and the command it covered. The caller writes them down.
    Excepted(Vec<(String, String)>),
}

/// The prefix that lets one command through, with its reason.
pub(crate) const EXCEPTION_VAR: &str = "SMIX_FENCE_EXCEPTION";

/// Judge every command in `text`, which may hold several lines and
/// `;` / `&&` / `|` chains. The first refusal decides.
pub(crate) fn judge(text: &str) -> Verdict {
    let mut excepted = Vec::new();
    for command in split_commands(text) {
        match judge_one(&words(&command), 0) {
            One::Pass => {}
            One::Lifecycle(what) => {
                let words = words(&command);
                match exception_reason(&words) {
                    Some(reason) if !reason.trim().is_empty() => {
                        excepted.push((reason, command.trim().to_string()));
                    }
                    Some(_) => {
                        return Verdict::Deny(format!(
                            "`{EXCEPTION_VAR}=` needs a reason that says why this \
                             one command goes around smix, in: {}",
                            command.trim()
                        ));
                    }
                    None => return Verdict::Deny(refusal(what, command.trim())),
                }
            }
        }
    }
    if excepted.is_empty() {
        Verdict::Allow
    } else {
        Verdict::Excepted(excepted)
    }
}

/// The lifecycle step a command takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    SimctlBoot,
    SimctlShutdown,
    SimctlErase,
    SimctlCreateOrDelete,
    EmulatorStart,
    EmulatorKill,
    AvdCreateOrDelete,
}

enum One {
    Pass,
    Lifecycle(Step),
}

fn refusal(step: Step, command: &str) -> String {
    let instead = match step {
        Step::SimctlBoot => "smix sim boot <alias|udid>",
        Step::SimctlShutdown => "smix sim shutdown <alias|udid>",
        Step::SimctlErase => "smix sim erase <alias|udid>",
        Step::SimctlCreateOrDelete => "smix sim exec <udid> <verb> … for a device smix knows",
        Step::EmulatorStart => "smix sim boot <alias|serial> (headless; --window to show it)",
        Step::EmulatorKill => "smix sim shutdown <serial>",
        Step::AvdCreateOrDelete => "an AVD made once, by hand, outside an agent session",
    };
    format!(
        "this starts, stops, erases or creates a device outside smix, and smix \
         keeps every device's life: who booted it, who may shut it down, its \
         PIN. Use: {instead}.\n\
         A device started any other way is refused when smix is asked to drive \
         it. If this one command really must go around smix, prefix it with \
         the reason, which is logged: {EXCEPTION_VAR}=\"<why>\" <command>.\n\
         refused: {command}"
    )
}

/// The value of a `SMIX_FENCE_EXCEPTION=` prefix on this command.
fn exception_reason(words: &[String]) -> Option<String> {
    words
        .iter()
        .take_while(|w| is_assignment(w) || is_wrapper(w))
        .find_map(|w| {
            w.strip_prefix(&format!("{EXCEPTION_VAR}="))
                .map(str::to_string)
        })
}

const WRAPPERS: &[&str] = &[
    "sudo",
    "env",
    "command",
    "exec",
    "nohup",
    "nice",
    "timeout",
    "stdbuf",
    "time",
    "xargs",
    "caffeinate",
];

fn is_wrapper(w: &str) -> bool {
    WRAPPERS.contains(&w)
}

fn is_assignment(w: &str) -> bool {
    w.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
    })
}

fn basename(w: &str) -> &str {
    w.rsplit('/').next().unwrap_or(w)
}

/// What the command at `words` runs, past assignments and wrappers, and
/// whether that is a lifecycle step. `depth` bounds `sh -c` recursion.
fn judge_one(words: &[String], depth: u8) -> One {
    let mut rest = words
        .iter()
        .map(String::as_str)
        .skip_while(|w| is_assignment(w));
    let mut program = rest.next();
    while let Some(p) = program
        && is_wrapper(basename(p))
    {
        // a wrapper's own flags, durations, priorities and env pairs
        program = rest.find(|w| {
            !(w.starts_with('-')
                || is_assignment(w)
                || w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        });
    }
    let Some(program) = program else {
        return One::Pass;
    };
    let args: Vec<&str> = rest.collect();
    match basename(program) {
        "xcrun" => {
            let tool = args.iter().position(|a| !a.starts_with('-'));
            match tool.map(|i| (basename(args[i]), &args[i + 1..])) {
                Some(("simctl", after)) => simctl(after),
                _ => One::Pass,
            }
        }
        "simctl" => simctl(&args),
        "emulator" => {
            let starts = args.iter().any(|a| *a == "-avd" || a.starts_with('@'));
            if starts {
                One::Lifecycle(Step::EmulatorStart)
            } else {
                One::Pass
            }
        }
        p if p.starts_with("qemu-system-") => One::Lifecycle(Step::EmulatorStart),
        "adb" => {
            let kills = args.windows(2).any(|w| w[0] == "emu" && w[1] == "kill");
            if kills {
                One::Lifecycle(Step::EmulatorKill)
            } else {
                One::Pass
            }
        }
        "avdmanager" => match args.iter().find(|a| !a.starts_with('-')) {
            Some(&("create" | "delete")) => One::Lifecycle(Step::AvdCreateOrDelete),
            _ => One::Pass,
        },
        "bash" | "sh" | "zsh" | "dash" if depth < 4 => match args.iter().position(|a| *a == "-c") {
            Some(i) if i + 1 < args.len() => nested(args[i + 1], depth),
            _ => One::Pass,
        },
        "eval" if depth < 4 => nested(&args.join(" "), depth),
        _ => One::Pass,
    }
}

fn nested(script: &str, depth: u8) -> One {
    for command in split_commands(script) {
        if let One::Lifecycle(step) = judge_one(&words(&command), depth + 1) {
            return One::Lifecycle(step);
        }
    }
    One::Pass
}

fn simctl(args: &[&str]) -> One {
    let mut it = args.iter();
    let verb = loop {
        match it.next() {
            Some(&"--set") => {
                it.next();
            }
            Some(a) if a.starts_with('-') => {}
            other => break other.copied(),
        }
    };
    match verb {
        Some("boot") => One::Lifecycle(Step::SimctlBoot),
        Some("shutdown") => One::Lifecycle(Step::SimctlShutdown),
        Some("erase") => One::Lifecycle(Step::SimctlErase),
        Some("create" | "delete" | "clone") => One::Lifecycle(Step::SimctlCreateOrDelete),
        _ => One::Pass,
    }
}

/// Split a shell text into simple commands at newlines, `;`, `&`, `|`,
/// parentheses and backticks outside quotes. Quotes are kept, so the
/// pieces still read as the commands they were.
pub(crate) fn split_commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => {
                quote = None;
                cur.push(c);
            }
            (Some(_), _) => cur.push(c),
            (None, '\\') => {
                escaped = true;
                cur.push(c);
            }
            (None, '\'' | '"') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, '\n' | ';' | '&' | '|' | '(' | ')' | '`') => {
                out.push(std::mem::take(&mut cur));
            }
            (None, _) => cur.push(c),
        }
    }
    out.push(cur);
    out.retain(|c| !c.trim().is_empty() && !c.trim_start().starts_with('#'));
    out
}

/// The words of one simple command, quotes removed.
pub(crate) fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut escaped = false;
    for c in command.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => cur.push(c),
            (None, '\\') => {
                escaped = true;
                started = true;
            }
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, _) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
