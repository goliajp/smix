//! A queue for a group of devices: one holder at a time, the rest in the
//! order they asked.
//!
//! The device ledger answers "who answers for this device". This answers
//! "whose turn is it" for a set of devices a team shares, where several
//! agents take turns and a turn spans many separate smix commands.
//!
//! A holder is known by a name the caller gives, not by a process: agents
//! that are subagents of one long-lived process share every ancestor, so
//! "the holder's process exited" never happens for the one that stopped.
//! What does tell a stopped holder from a busy one is silence: each device
//! command the holder runs renews the hold, and a hold not renewed within
//! its idle limit is released. A caller that does have a process of its
//! own can name it, and the hold ends when that process does.
//!
//! The judgements are pure functions of the state, the time and a liveness
//! probe, so the queue's rules are tested without a clock or a process.

use crate::ProcIdentity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod dir;
pub use dir::ScopeDir;

/// How long a hold lasts without a command from its holder, unless the
/// holder said otherwise when it took it.
pub const DEFAULT_IDLE_SECS: u64 = 20 * 60;

/// How many automatic releases a scope remembers, for `status`.
const RELEASES_KEPT: usize = 10;

/// One scope's state, as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    /// The devices this scope covers, as they were named when declared:
    /// aliases, AVD names, UDIDs or serials. Resolved when compared.
    #[serde(default)]
    pub devices: Vec<String>,
    /// Whether its devices may be driven only by its holder, so that with
    /// nobody holding it nobody drives them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_holder: bool,
    /// Who has it now.
    #[serde(default)]
    pub holder: Option<Hold>,
    /// Who is waiting, first in line first.
    #[serde(default)]
    pub queue: Vec<Waiter>,
    /// The most recent holds that ended without their holder releasing
    /// them, newest last.
    #[serde(default)]
    pub released: Vec<Released>,
}

/// A hold on a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    pub holder: String,
    /// When it was granted, RFC 3339.
    pub since: String,
    /// The holder's last command or renewal, RFC 3339.
    pub last_seen: String,
    /// Released after this long without a command from the holder.
    pub idle_secs: u64,
    /// What the holder said about when it will be done, free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// A process whose exit ends the hold, when the holder named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<ProcIdentity>,
}

/// Somebody waiting for a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waiter {
    pub holder: String,
    /// When it joined the queue, RFC 3339.
    pub since: String,
    /// The `acquire --wait` process doing the waiting. When it is gone,
    /// nobody is waiting, whatever the queue says.
    pub waiting: ProcIdentity,
    pub idle_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<ProcIdentity>,
}

/// A hold that ended without its holder releasing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Released {
    pub holder: String,
    /// When it was noticed, RFC 3339.
    pub at: String,
    pub reason: String,
}

/// What somebody asked for.
#[derive(Debug, Clone)]
pub struct Request {
    pub holder: String,
    /// Queue when somebody else holds it, rather than answer at once.
    pub wait: bool,
    pub idle_secs: u64,
    pub until: Option<String>,
    pub pid: Option<ProcIdentity>,
    /// The process asking, which is the one that waits.
    pub asking: ProcIdentity,
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Granted {
    /// The requester holds the scope.
    Held,
    /// The requester is in the queue at this place, 1 being next.
    Queued(usize),
    /// Somebody else holds it and the requester did not ask to wait.
    Busy,
}

fn parse(t: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(t)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Whole seconds from `then` to `now`, or `None` when `then` is unreadable.
fn age(then: &str, now: DateTime<Utc>) -> Option<u64> {
    parse(then).map(|t| (now - t).num_seconds().max(0) as u64)
}

impl Scope {
    /// End holds whose holder has gone quiet or whose named process has
    /// exited, drop waiters whose waiting process is gone, and give the
    /// scope to the first in line when nobody holds it.
    ///
    /// `alive` says whether a recorded process is still the one running.
    /// Returns whether anything changed.
    pub fn settle(&mut self, now: DateTime<Utc>, alive: &dyn Fn(&ProcIdentity) -> bool) -> bool {
        let mut changed = false;
        if let Some(h) = &self.holder {
            let reason = match (&h.pid, age(&h.last_seen, now)) {
                (Some(p), _) if !alive(p) => {
                    Some(format!("its process {} ({}) exited", p.pid, p.cmd))
                }
                (_, Some(quiet)) if quiet > h.idle_secs => Some(format!(
                    "no command from it for {} min (its limit is {} min)",
                    quiet / 60,
                    h.idle_secs / 60
                )),
                (_, None) => Some("its last command's time could not be read".to_string()),
                _ => None,
            };
            if let Some(reason) = reason {
                self.released.push(Released {
                    holder: h.holder.clone(),
                    at: now.to_rfc3339(),
                    reason,
                });
                let excess = self.released.len().saturating_sub(RELEASES_KEPT);
                self.released.drain(..excess);
                self.holder = None;
                changed = true;
            }
        }
        let before = self.queue.len();
        self.queue.retain(|w| alive(&w.waiting));
        changed |= self.queue.len() != before;
        if self.holder.is_none() && !self.queue.is_empty() {
            let w = self.queue.remove(0);
            let t = now.to_rfc3339();
            self.holder = Some(Hold {
                holder: w.holder,
                since: t.clone(),
                last_seen: t,
                idle_secs: w.idle_secs,
                until: w.until,
                pid: w.pid,
            });
            changed = true;
        }
        changed
    }

    /// Ask for the scope. Call [`Self::settle`] first.
    pub fn acquire(&mut self, req: &Request, now: DateTime<Utc>) -> Granted {
        let t = now.to_rfc3339();
        if let Some(h) = &mut self.holder {
            if h.holder == req.holder {
                h.last_seen = t;
                return Granted::Held;
            }
        } else if self.queue.is_empty() {
            self.holder = Some(Hold {
                holder: req.holder.clone(),
                since: t.clone(),
                last_seen: t,
                idle_secs: req.idle_secs,
                until: req.until.clone(),
                pid: req.pid.clone(),
            });
            return Granted::Held;
        }
        if let Some(i) = self.queue.iter().position(|w| w.holder == req.holder) {
            // the same holder asking again keeps its place; when it asks
            // to wait, the newest process asking is the one waiting now,
            // and a question without `wait` changes nothing
            if req.wait {
                let w = &mut self.queue[i];
                w.waiting = req.asking.clone();
                w.idle_secs = req.idle_secs;
                w.until.clone_from(&req.until);
                w.pid.clone_from(&req.pid);
            }
            return Granted::Queued(i + 1);
        }
        if !req.wait {
            return Granted::Busy;
        }
        self.queue.push(Waiter {
            holder: req.holder.clone(),
            since: t,
            waiting: req.asking.clone(),
            idle_secs: req.idle_secs,
            until: req.until.clone(),
            pid: req.pid.clone(),
        });
        Granted::Queued(self.queue.len())
    }

    /// Give the scope up, or leave the queue. Returns whether `holder` was
    /// holding or waiting.
    pub fn release(&mut self, holder: &str) -> bool {
        let held = self.holder.as_ref().is_some_and(|h| h.holder == holder);
        if held {
            self.holder = None;
        }
        let before = self.queue.len();
        self.queue.retain(|w| w.holder != holder);
        held || self.queue.len() != before
    }

    /// Leave the queue, when `asking` is still the process waiting there:
    /// a waiter that gives up does not take away a place another process
    /// of the same holder has taken over. Returns whether it left.
    pub fn give_up(&mut self, holder: &str, asking: &ProcIdentity) -> bool {
        let before = self.queue.len();
        self.queue
            .retain(|w| !(w.holder == holder && &w.waiting == asking));
        self.queue.len() != before
    }

    /// Record a command or renewal from `holder`. Returns whether it holds
    /// the scope.
    pub fn touch(&mut self, holder: &str, now: DateTime<Utc>) -> bool {
        match &mut self.holder {
            Some(h) if h.holder == holder => {
                h.last_seen = now.to_rfc3339();
                true
            }
            _ => false,
        }
    }
}

/// The scope that keeps a command from `me` off `targets`, or `None`.
///
/// `targets` are every name the command's devices are known by; `ids`
/// gives the same for a scope's declared device. A scope covering one of
/// the targets refuses when somebody other than `me` holds it, and is
/// renewed when `me` does; one that requires a holder refuses while nobody
/// holds it. A scope that cannot be read refuses: answering
/// yes there would make the check absent on any machine whose scopes are
/// broken.
pub fn refusal(
    dir: &ScopeDir,
    targets: &std::collections::BTreeSet<String>,
    ids: &dyn Fn(&str) -> std::collections::BTreeSet<String>,
    me: Option<&str>,
    alive: &dyn Fn(&ProcIdentity) -> bool,
) -> Option<String> {
    if targets.is_empty() {
        return None;
    }
    for name in dir.names() {
        let read = dir.update(&name, |s| {
            let now = Utc::now();
            s.settle(now, alive);
            if !s.devices.iter().any(|m| !ids(m).is_disjoint(targets)) {
                return None;
            }
            match (&s.holder, me) {
                (None, _) => s.require_holder.then(|| unheld_text(&name, me)),
                (Some(h), Some(me)) if h.holder == me => {
                    s.touch(me, now);
                    None
                }
                (Some(h), _) => Some(refusal_text(&name, h, me)),
            }
        });
        match read {
            Ok(None) => {}
            Ok(Some(why)) => return Some(why),
            Err(e) => return Some(format!("scope {name} could not be read: {e}")),
        }
    }
    None
}

/// The environment variable a device command names its holder in.
pub const HOLDER_ENV: &str = "SMIX_LEASE_HOLDER";

/// A stored RFC 3339 time as this machine's clock reads it, to the second.
#[must_use]
pub fn shown(t: &str) -> String {
    DateTime::parse_from_rfc3339(t).map_or_else(
        |_| t.to_string(),
        |d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        },
    )
}

fn unheld_text(name: &str, me: Option<&str>) -> String {
    let who = me.unwrap_or("<you>");
    format!(
        "this device is in scope {name}, which is driven only by whoever holds it, \
         and nobody does. Take it:\n  smix lease acquire --scope {name} --holder {who} --wait\n\
         then run device commands with {HOLDER_ENV}={who}."
    )
}

fn refusal_text(name: &str, h: &Hold, me: Option<&str>) -> String {
    let who = match me {
        Some(me) => format!("you run as {me}"),
        None => format!("{HOLDER_ENV} is not set"),
    };
    let until = h
        .until
        .as_deref()
        .map(|u| format!(", until {u}"))
        .unwrap_or_default();
    format!(
        "this device is in scope {name}, which {} holds (since {}{until}) — {who}. \
         Wait for it:\n  smix lease acquire --scope {name} --holder <you> --wait\n\
         then run device commands with {HOLDER_ENV}=<you>. \
         `smix lease status --scope {name}` shows the queue.",
        h.holder,
        shown(&h.since)
    )
}

/// A scope name is a file name here, so it is kept to what is safe in one.
pub fn valid_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "scope name {name:?}: use letters, digits, '-', '_' and '.', not starting with '.'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: format!("start-{pid}"),
            cmd: format!("agent {pid}"),
        }
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + secs, 0).expect("time")
    }

    fn ask(holder: &str, pid: u32, wait: bool) -> Request {
        Request {
            holder: holder.into(),
            wait,
            idle_secs: DEFAULT_IDLE_SECS,
            until: None,
            pid: None,
            asking: proc(pid),
        }
    }

    fn all_alive(_: &ProcIdentity) -> bool {
        true
    }

    #[test]
    fn the_scope_goes_in_the_order_it_was_asked_for() {
        let mut s = Scope::default();
        assert_eq!(s.acquire(&ask("a", 1, true), at(0)), Granted::Held);
        assert_eq!(s.acquire(&ask("b", 2, true), at(1)), Granted::Queued(1));
        assert_eq!(s.acquire(&ask("c", 3, true), at(2)), Granted::Queued(2));
        // c asks again first: asking more often does not move it ahead
        assert_eq!(s.acquire(&ask("c", 3, true), at(3)), Granted::Queued(2));
        assert!(s.release("a"));
        s.settle(at(4), &all_alive);
        assert_eq!(s.holder.as_ref().unwrap().holder, "b");
        assert_eq!(s.acquire(&ask("c", 3, true), at(5)), Granted::Queued(1));
        assert!(s.release("b"));
        s.settle(at(6), &all_alive);
        assert_eq!(s.holder.as_ref().unwrap().holder, "c");
        assert!(s.queue.is_empty());
    }

    #[test]
    fn one_holder_has_one_place_however_many_times_it_asks() {
        let mut s = Scope::default();
        s.acquire(&ask("a", 1, true), at(0));
        for pid in [2, 3, 4] {
            assert_eq!(s.acquire(&ask("b", pid, true), at(1)), Granted::Queued(1));
        }
        assert_eq!(s.queue.len(), 1);
        assert_eq!(
            s.queue[0].waiting.pid, 4,
            "the newest asker is the one waiting"
        );
        assert_eq!(s.acquire(&ask("b", 5, false), at(1)), Granted::Queued(1));
        assert_eq!(
            s.queue[0].waiting.pid, 4,
            "asking without waiting takes nothing over"
        );
        assert!(
            !s.give_up("b", &proc(2)),
            "an earlier asker cannot take the place away"
        );
        assert!(s.give_up("b", &proc(4)));
        assert!(s.queue.is_empty());
        assert_eq!(s.acquire(&ask("a", 9, true), at(2)), Granted::Held);
    }

    #[test]
    fn without_wait_a_held_scope_answers_busy_and_queues_nobody() {
        let mut s = Scope::default();
        s.acquire(&ask("a", 1, false), at(0));
        assert_eq!(s.acquire(&ask("b", 2, false), at(1)), Granted::Busy);
        assert!(s.queue.is_empty());
    }

    #[test]
    fn a_quiet_holder_is_released_and_the_next_in_line_gets_it() {
        let mut s = Scope::default();
        s.acquire(&ask("a", 1, true), at(0));
        s.acquire(&ask("b", 2, true), at(1));
        assert!(
            !s.settle(at(DEFAULT_IDLE_SECS as i64), &all_alive),
            "at the limit it still holds"
        );
        assert!(s.touch("a", at(DEFAULT_IDLE_SECS as i64)));
        assert!(
            !s.settle(at(DEFAULT_IDLE_SECS as i64 * 2 - 1), &all_alive),
            "a command renews it"
        );
        assert!(s.settle(at(DEFAULT_IDLE_SECS as i64 * 2 + 1), &all_alive));
        assert_eq!(s.holder.as_ref().unwrap().holder, "b");
        let r = s.released.last().unwrap();
        assert_eq!(r.holder, "a");
        assert!(
            r.reason.contains("no command from it for 20 min"),
            "{}",
            r.reason
        );
    }

    #[test]
    fn a_holder_with_a_process_is_released_when_it_exits() {
        let mut s = Scope::default();
        let mut req = ask("a", 1, true);
        req.pid = Some(proc(77));
        s.acquire(&req, at(0));
        assert!(!s.settle(at(5), &all_alive));
        assert!(s.settle(at(6), &|p: &ProcIdentity| p.pid != 77));
        assert!(s.holder.is_none());
        assert!(
            s.released[0].reason.contains("process 77"),
            "{}",
            s.released[0].reason
        );
    }

    #[test]
    fn a_waiter_whose_process_is_gone_leaves_the_queue() {
        let mut s = Scope::default();
        s.acquire(&ask("a", 1, true), at(0));
        s.acquire(&ask("b", 2, true), at(1));
        s.acquire(&ask("c", 3, true), at(2));
        assert!(s.settle(at(3), &|p: &ProcIdentity| p.pid != 2));
        assert_eq!(s.queue.len(), 1);
        assert_eq!(s.queue[0].holder, "c");
        assert_eq!(
            s.holder.as_ref().unwrap().holder,
            "a",
            "the holder is not touched"
        );
    }

    #[test]
    fn touch_and_release_answer_only_for_the_holder() {
        let mut s = Scope::default();
        s.acquire(&ask("a", 1, true), at(0));
        assert!(!s.touch("b", at(1)));
        assert!(!s.release("b"));
        assert_eq!(s.holder.as_ref().unwrap().holder, "a");
    }

    #[test]
    fn only_the_latest_releases_are_kept() {
        let mut s = Scope::default();
        for i in 0..15 {
            s.acquire(&ask(&format!("h{i}"), 1, true), at(i * 10_000));
            s.settle(at(i * 10_000 + 5_000), &all_alive);
        }
        assert_eq!(s.released.len(), RELEASES_KEPT);
        assert_eq!(s.released.last().unwrap().holder, "h14");
    }

    #[test]
    fn the_store_round_trips_under_its_lock() {
        let t = tempfile::tempdir().expect("tempdir");
        let dir = ScopeDir::at(t.path().join("scopes"));
        assert!(dir.names().is_empty());
        dir.update("insight", |s| {
            s.devices = vec!["sim-insight".into()];
            s.acquire(&ask("a", 1, true), at(0))
        })
        .expect("write");
        let held = dir.update("insight", |s| s.holder.clone()).expect("read");
        assert_eq!(held.unwrap().holder, "a");
        assert_eq!(dir.names(), vec!["insight".to_string()]);
    }

    #[test]
    fn a_command_on_a_held_scope_is_refused_unless_it_is_the_holders() {
        let t = tempfile::tempdir().expect("tempdir");
        let dir = ScopeDir::at(t.path());
        let ids = |n: &str| std::collections::BTreeSet::from([n.to_string()]);
        let target = std::collections::BTreeSet::from(["dev-1".to_string()]);
        let other = std::collections::BTreeSet::from(["dev-2".to_string()]);
        dir.update("s", |s| s.devices = vec!["dev-1".into()])
            .unwrap();
        assert!(
            refusal(&dir, &target, &ids, None, &all_alive).is_none(),
            "free"
        );
        dir.update("s", |s| s.acquire(&ask("a", 1, true), Utc::now()))
            .unwrap();
        let why = refusal(&dir, &target, &ids, Some("b"), &all_alive).expect("refused");
        assert!(
            why.contains("which a holds") && why.contains("you run as b"),
            "{why}"
        );
        assert!(
            refusal(&dir, &target, &ids, None, &all_alive)
                .unwrap()
                .contains("not set")
        );
        assert!(
            refusal(&dir, &target, &ids, Some("a"), &all_alive).is_none(),
            "the holder"
        );
        assert!(
            refusal(&dir, &other, &ids, Some("b"), &all_alive).is_none(),
            "outside it"
        );
        assert!(refusal(&dir, &Default::default(), &ids, Some("b"), &all_alive).is_none());
    }

    #[test]
    fn a_scope_name_cannot_leave_its_directory() {
        for bad in ["", "../x", "a/b", ".hidden", "a b"] {
            assert!(valid_name(bad).is_err(), "{bad:?}");
        }
        assert!(valid_name("insight-01_a.b").is_ok());
    }
}
