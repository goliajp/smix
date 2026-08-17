//! Which device this MCP session is driving, and since when.
//!
//! The server used to read `SMIX_UDID` once at startup and bind to it for
//! the life of the process. That put the choice of device in the client's
//! configuration file — decided before the conversation started, by
//! someone editing JSON — and left no way to answer "use the other
//! simulator" without restarting the client. It also meant a machine with
//! no such variable set got a server whose tools all failed on the same
//! missing binding, saying only that they could not connect.
//!
//! So binding is session state, and the environment is a default for it
//! rather than the only source.

use std::sync::Mutex;

/// The device a session is currently driving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    /// Simulator UDID.
    pub udid: String,
    /// Port its runner answers on.
    pub port: u16,
}

/// Session-scoped device binding.
///
/// Interior mutability because rmcp hands each tool `&self`: the binding
/// has to be changeable from inside a tool call, which is the entire
/// point — `smix_use` is a tool.
#[derive(Debug, Default)]
pub struct SessionState {
    bound: Mutex<Option<Bound>>,
}

/// What to tell a caller that has not chosen a device yet.
///
/// Names the tool that fixes it. The old failure was a connection error
/// from whichever tool happened to be called first, which describes a
/// symptom of the missing binding rather than the binding.
pub const UNBOUND_HINT: &str = "no device is bound to this session — call smix_use with a UDID first \
     (smix_devices lists what is available)";

impl SessionState {
    /// A session with nothing bound.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A session pre-bound from the environment, when it said anything.
    ///
    /// A default, not a requirement: absent variables leave the session
    /// unbound and every tool pointing at `smix_use`, rather than making
    /// half the server unusable.
    #[must_use]
    pub fn from_env(udid: Option<String>, port: u16) -> Self {
        let state = Self::new();
        if let Some(udid) = udid {
            state.bind(Bound { udid, port });
        }
        state
    }

    /// Bind to a device, replacing any previous binding.
    ///
    /// Returns what was bound before, so the caller can decide whether the
    /// old runner needs releasing — this type tracks the choice, it does
    /// not run teardown.
    pub fn bind(&self, next: Bound) -> Option<Bound> {
        let mut slot = self.bound.lock().expect("session lock");
        slot.replace(next)
    }

    /// The current binding.
    #[must_use]
    pub fn current(&self) -> Option<Bound> {
        self.bound.lock().expect("session lock").clone()
    }

    /// Drop the binding, returning what it was.
    pub fn release(&self) -> Option<Bound> {
        self.bound.lock().expect("session lock").take()
    }

    /// The current binding, or the error a tool should return without one.
    pub fn require(&self) -> Result<Bound, &'static str> {
        self.current().ok_or(UNBOUND_HINT)
    }
}

/// A read-only JSON report of what this session is bound to, for the
/// `smix_session_state` MCP tool and the `smix session state` CLI. It is
/// the one report answerable with nothing bound — that is the point:
/// "what am I driving?" before any tool that needs a binding.
pub fn session_state_report(bound: Option<&Bound>) -> String {
    let value = match bound {
        None => serde_json::json!({ "bound": false }),
        Some(b) => serde_json::json!({
            "bound": true,
            "udid": b.udid,
            "port": b.port,
        }),
    };
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_unbound_reports_bound_false() {
        let json = session_state_report(None);
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(v["bound"], serde_json::json!(false), "got: {json}");
        assert!(v.get("udid").is_none(), "no udid when unbound: {json}");
        assert!(v.get("port").is_none(), "no port when unbound: {json}");
    }

    #[test]
    fn session_state_bound_reports_udid_and_port() {
        let json = session_state_report(Some(&Bound {
            udid: "AAAA".into(),
            port: 22087,
        }));
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(v["bound"], serde_json::json!(true), "got: {json}");
        assert_eq!(v["udid"], serde_json::json!("AAAA"), "got: {json}");
        assert_eq!(v["port"], serde_json::json!(22087), "got: {json}");
    }

    #[test]
    fn an_unbound_session_names_the_tool_that_binds_one() {
        let s = SessionState::new();
        let err = s.require().expect_err("nothing bound yet");
        assert!(err.contains("smix_use"), "got: {err}");
        assert!(err.contains("smix_devices"), "got: {err}");
    }

    #[test]
    fn binding_replaces_and_hands_back_the_previous_device() {
        let s = SessionState::new();
        assert!(
            s.bind(Bound {
                udid: "AAAA".into(),
                port: 22087
            })
            .is_none()
        );

        let previous = s.bind(Bound {
            udid: "BBBB".into(),
            port: 22088,
        });
        assert_eq!(
            previous,
            Some(Bound {
                udid: "AAAA".into(),
                port: 22087
            })
        );
        assert_eq!(s.current().expect("bound").udid, "BBBB");
    }

    #[test]
    fn releasing_returns_to_the_same_error_a_fresh_session_gives() {
        let s = SessionState::new();
        s.bind(Bound {
            udid: "AAAA".into(),
            port: 22087,
        });
        assert_eq!(s.release().expect("was bound").udid, "AAAA");
        assert!(s.current().is_none());
        assert_eq!(s.require().expect_err("unbound again"), UNBOUND_HINT);
    }

    #[test]
    fn the_environment_is_a_default_not_a_requirement() {
        // Absent, the session is simply unbound — the tools work, they
        // just need a device chosen. Present, it starts bound, which is
        // what a client with an existing config keeps getting.
        assert!(SessionState::from_env(None, 22087).current().is_none());
        let pre = SessionState::from_env(Some("AAAA".into()), 22099);
        assert_eq!(
            pre.current().expect("bound"),
            Bound {
                udid: "AAAA".into(),
                port: 22099
            }
        );
    }
}
