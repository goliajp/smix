//! Federation scheduling core: shard flows across devices on remote
//! nodes, pure logic only (no ssh, no file IO — execution is C3+).
//!
//! The node roster lives in `.smix/nodes.yaml` (same direct-read shape
//! as `.smix/config.yaml`; discovery via `workspace_root` happens at the
//! consuming layer, not here). Nodes list simulators/emulators only —
//! the §9#1 invariant holds across machines. A device ref in the roster
//! is an alias/UDID for the *remote* node's registry; the scheduler
//! never resolves a remote ref locally, it forwards it verbatim.

use serde::Deserialize;

/// One remote node in the roster: how to reach it, where its smix repo
/// lives, and which of its registered devices the federation may use.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NodeSpec {
    pub name: String,
    pub host: String,
    pub repo: String,
    pub devices: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum NodesError {
    #[error("nodes yaml is malformed: {message}")]
    Malformed { message: String },
    #[error("nodes yaml lists no nodes")]
    Empty,
    #[error("node '{node}' lists no devices")]
    EmptyDevices { node: String },
    #[error("duplicate node name '{name}'")]
    DuplicateName { name: String },
}

#[derive(Deserialize)]
struct NodesFile {
    nodes: Vec<NodeSpec>,
}

/// Parse the `.smix/nodes.yaml` roster. Hand-written yaml is a trust
/// boundary, so the shape is validated here: a non-empty roster, every
/// node with at least one device, node names unique.
pub fn parse_nodes(yaml: &str) -> Result<Vec<NodeSpec>, NodesError> {
    let file: NodesFile = serde_norway::from_str(yaml)
        .map_err(|e| NodesError::Malformed { message: e.to_string() })?;
    if file.nodes.is_empty() {
        return Err(NodesError::Empty);
    }
    let mut seen = std::collections::HashSet::new();
    for node in &file.nodes {
        if node.devices.is_empty() {
            return Err(NodesError::EmptyDevices { node: node.name.clone() });
        }
        if !seen.insert(node.name.as_str()) {
            return Err(NodesError::DuplicateName { name: node.name.clone() });
        }
    }
    Ok(file.nodes)
}

/// Flatten the roster into device slots, in listing order: node 0's
/// devices first, then node 1's, and so on. A slot is `(node index,
/// device ref)` — the unit the round-robin assigns flows to.
#[must_use]
pub fn expand_slots(nodes: &[NodeSpec]) -> Vec<(usize, String)> {
    nodes
        .iter()
        .enumerate()
        .flat_map(|(node, spec)| spec.devices.iter().map(move |d| (node, d.clone())))
        .collect()
}

/// One slot's share of the batch: which node, which device ref on that
/// node, and the flow indices it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotAssignment {
    pub node: usize,
    pub device_ref: String,
    pub flows: Vec<usize>,
}

/// Assign flows round-robin over all slots across all nodes. The flow
/// buckets are `parallel::shard_flows` verbatim — one round-robin
/// semantic, maintained in one place — zipped back onto the slots.
#[must_use]
pub fn assign_flows(flow_count: usize, slots: &[(usize, String)]) -> Vec<SlotAssignment> {
    crate::parallel::shard_flows(flow_count, slots.len())
        .into_iter()
        .zip(slots.iter().cloned())
        .map(|(flows, (node, device_ref))| SlotAssignment {
            node,
            device_ref,
            flows,
        })
        .collect()
}

/// ssh's own exit code for a transport failure (connection refused,
/// auth denied, host unreachable). Disjoint from smix's exit space
/// {0,1,2,3,4,5,6,130,143}, and the max of both, so the worst-of-nodes
/// aggregate is fail-safe: transport loss always wins.
pub const SSH_TRANSPORT_EXIT: u8 = 255;

#[must_use]
pub fn is_transport_failure(code: u8) -> bool {
    code == SSH_TRANSPORT_EXIT
}

/// POSIX single-quote escaping. Always quotes — bare-safe input too —
/// so the remote command has one deterministic shape.
#[must_use]
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Build the argv for `ssh` (the `"ssh"` word itself excluded, the same
/// convention as `child_argv` excluding the exe — spawning is C3's leg):
/// batch-mode options, the node's host, and one remote command string.
///
/// The remote command is `parallel::child_argv` verbatim — the smix-side
/// argv is composed in one place, federation only wraps the SSH skin —
/// with the data tokens (flows, device ref) shell-quoted, prefixed by a
/// `cd` into the node's repo, and `--format json` appended
/// unconditionally: the merge loop reads the remote stdout as JSON
/// lines, and the single-machine passthrough never carries `--format`.
/// The device ref is the remote registry's alias, forwarded verbatim.
#[must_use]
pub fn remote_argv(
    node: &NodeSpec,
    flows: &[String],
    device_ref: &str,
    passthrough: &[String],
) -> Vec<String> {
    let quoted_flows: Vec<String> = flows.iter().map(|f| shell_quote(f)).collect();
    let smix_argv = crate::parallel::child_argv(&quoted_flows, &shell_quote(device_ref), passthrough);
    let remote = format!(
        "cd {} && target/release/smix {} --format json",
        shell_quote(&node.repo),
        smix_argv.join(" ")
    );
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        node.host.clone(),
        remote,
    ]
}

/// The freshness stamp, touched after a successful `cargo build` on the
/// node. `target/` is excluded from the source rsync, so a sync can
/// never forge freshness — only a real rebuild lays the stamp.
pub const FED_BUILD_STAMP: &str = "target/.smix-fed-stamp";

/// One flow's report line from a remote `--format json` stdout. `raw`
/// keeps the whole line as parsed — the C4 merge layer consumes it;
/// C3 does not reshape it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowReport {
    pub flow: String,
    pub outcome: String,
    pub raw: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("remote stdout line is not JSON (protocol violation): {line}")]
    NotJson { line: String },
    #[error("remote report line is missing '{field}': {line}")]
    MissingField { field: &'static str, line: String },
}

/// Parse a remote `--format json` stdout into per-flow reports. The
/// remote stdout is a trust boundary: under `--format json` it must
/// carry only JSON lines (noise is stderr's — verified on the wire),
/// so any non-JSON line is a protocol violation surfaced as an error,
/// never skipped. Blank lines between reports are not violations.
pub fn parse_report_lines(stdout: &str) -> Result<Vec<FlowReport>, ReportError> {
    let mut reports = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let raw: serde_json::Value = serde_json::from_str(line)
            .map_err(|_| ReportError::NotJson { line: line.to_string() })?;
        let field = |name: &'static str| -> Result<String, ReportError> {
            raw.get(name)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or(ReportError::MissingField { field: name, line: line.to_string() })
        };
        let flow = field("flow")?;
        let outcome = field("runOutcome")?;
        reports.push(FlowReport { flow, outcome, raw });
    }
    Ok(reports)
}

/// Build the argv for the readiness gate — a read-only probe (repair,
/// i.e. rebuilding, is the sync script's job, never the gate's). Same
/// ssh conventions as `remote_argv`: no `"ssh"` word, batch mode, one
/// quoted remote command. The order is fail-safe: a missing stamp
/// fails `test -f` before anything else, so "never rebuilt" reads as
/// stale, not fresh.
#[must_use]
pub fn readiness_argv(node: &NodeSpec) -> Vec<String> {
    let remote = format!(
        "cd {} && test -f {FED_BUILD_STAMP} && test -x target/release/smix && \
         [ -z \"$(find crates -name '*.rs' -newer {FED_BUILD_STAMP})\" ]",
        shell_quote(&node.repo),
    );
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        node.host.clone(),
        remote,
    ]
}

/// A remote command's full result: exit code plus both streams,
/// captured whole. SSH keeps the remote stdout/stderr split intact
/// on the wire, and federation depends on that — stdout is the JSON
/// report channel, stderr the noise channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOutput {
    pub exit: u8,
    pub stdout: String,
    pub stderr: String,
}

/// Spawn `ssh` with the given argv and capture everything. No retry,
/// no timeout here — ssh's own transport failures surface as the 255
/// sentinel and win the worst-of-nodes aggregate; wrapping them would
/// hide exactly the signal the merge loop needs. A signal-killed ssh
/// has no exit code and maps to 1, the same shape `run_parallel` uses.
pub fn run_ssh(argv: &[String]) -> std::io::Result<RemoteOutput> {
    let out = std::process::Command::new("ssh").args(argv).output()?;
    Ok(RemoteOutput {
        exit: out.status.code().map_or(1, |c| c.clamp(0, 255) as u8),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENTED_NODES_YAML: &str = "\
nodes:
  - name: mini
    host: mini
    repo: /Users/doracawl/workspace/goliajp/smix
    devices: [sim-smix-001]
  - name: studio
    host: studio.local
    repo: /Users/doracawl/smix
    devices: [sim-smix-002, sim-smix-003]
";

    #[test]
    fn parses_the_documented_nodes_yaml_shape() {
        let nodes = parse_nodes(DOCUMENTED_NODES_YAML).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "mini");
        assert_eq!(nodes[0].host, "mini");
        assert_eq!(nodes[0].repo, "/Users/doracawl/workspace/goliajp/smix");
        assert_eq!(nodes[0].devices, vec!["sim-smix-001"]);
        assert_eq!(nodes[1].name, "studio");
        assert_eq!(nodes[1].host, "studio.local");
        assert_eq!(nodes[1].repo, "/Users/doracawl/smix");
        assert_eq!(nodes[1].devices, vec!["sim-smix-002", "sim-smix-003"]);
    }

    #[test]
    fn rejects_a_node_without_devices() {
        let yaml = "\
nodes:
  - name: mini
    host: mini
    repo: /Users/doracawl/workspace/goliajp/smix
    devices: []
";
        let err = parse_nodes(yaml).unwrap_err();
        assert!(matches!(&err, NodesError::EmptyDevices { node } if node == "mini"));
        assert!(err.to_string().contains("mini"));
    }

    fn roster(specs: &[(&str, &[&str])]) -> Vec<NodeSpec> {
        specs
            .iter()
            .map(|(name, devices)| NodeSpec {
                name: (*name).to_string(),
                host: (*name).to_string(),
                repo: format!("/repo/{name}"),
                devices: devices.iter().map(|d| (*d).to_string()).collect(),
            })
            .collect()
    }

    #[test]
    fn slots_flatten_nodes_in_listing_order() {
        let nodes = roster(&[("a", &["a1", "a2"]), ("b", &["b1"])]);
        assert_eq!(
            expand_slots(&nodes),
            vec![
                (0, "a1".to_string()),
                (0, "a2".to_string()),
                (1, "b1".to_string())
            ]
        );
    }

    #[test]
    fn flows_round_robin_over_all_slots_across_nodes() {
        let nodes = roster(&[("a", &["a1", "a2"]), ("b", &["b1"])]);
        let slots = expand_slots(&nodes);
        let assignments = assign_flows(5, &slots);
        assert_eq!(
            assignments,
            vec![
                SlotAssignment {
                    node: 0,
                    device_ref: "a1".to_string(),
                    flows: vec![0, 3]
                },
                SlotAssignment {
                    node: 0,
                    device_ref: "a2".to_string(),
                    flows: vec![1, 4]
                },
                SlotAssignment {
                    node: 1,
                    device_ref: "b1".to_string(),
                    flows: vec![2]
                },
            ]
        );
        // Verbatim reuse of the single-machine round-robin: the flow
        // buckets are exactly shard_flows(5, 3).
        let buckets: Vec<Vec<usize>> = assignments.into_iter().map(|a| a.flows).collect();
        assert_eq!(buckets, crate::parallel::shard_flows(5, 3));
    }

    #[test]
    fn single_node_single_device_degenerates_to_the_sequential_order() {
        let nodes = roster(&[("a", &["a1"])]);
        let assignments = assign_flows(3, &expand_slots(&nodes));
        assert_eq!(
            assignments,
            vec![SlotAssignment {
                node: 0,
                device_ref: "a1".to_string(),
                flows: vec![0, 1, 2]
            }]
        );
    }

    #[test]
    fn remote_argv_wraps_child_argv_in_ssh_with_explicit_json_format() {
        let node = NodeSpec {
            name: "mini".to_string(),
            host: "mini".to_string(),
            repo: "/Users/doracawl/workspace/goliajp/smix".to_string(),
            devices: vec!["sim-smix-001".to_string()],
        };
        let argv = remote_argv(
            &node,
            &["a.yaml".to_string()],
            "sim-smix-001",
            &["--no-launch".to_string()],
        );
        assert_eq!(
            argv,
            vec![
                "-o".to_string(),
                "BatchMode=yes".to_string(),
                "mini".to_string(),
                "cd '/Users/doracawl/workspace/goliajp/smix' && target/release/smix \
                 run 'a.yaml' --device 'sim-smix-001' --no-launch --format json"
                    .to_string(),
            ]
        );
        let remote = &argv[3];
        assert!(remote.contains("--format json"));
        assert!(!remote.contains("--parallel"));
        assert!(!remote.contains("--also-device"));
    }

    #[test]
    fn shell_quoting_survives_spaces_and_single_quotes() {
        assert_eq!(shell_quote("flows/a b.yaml"), "'flows/a b.yaml'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
        // Bare-safe input is quoted too — one deterministic shape, no
        // "does this need quoting" branch.
        assert_eq!(shell_quote("a.yaml"), "'a.yaml'");
    }

    #[test]
    fn transport_failure_255_wins_the_aggregate() {
        assert!(is_transport_failure(SSH_TRANSPORT_EXIT));
        for smix_code in [0u8, 1, 2, 3, 4, 5, 6, 130, 143] {
            assert!(!is_transport_failure(smix_code));
        }
        // Worst-of-nodes is the same aggregate the single-machine batch
        // uses; 255 sits above every smix code, so transport loss can
        // never be masked by a flow failure.
        assert_eq!(crate::parallel::aggregate_exit(&[0, 255, 2]), 255);
    }

    #[test]
    fn parses_one_report_line_per_flow() {
        let stdout = concat!(
            r#"{"flow":"scripts/release/stress-corpus/launch-and-capture.yaml","runOutcome":"success","warnings":[],"steps":[]}"#,
            "\n",
            r#"{"flow":"scripts/release/stress-corpus/screenshot-twice.yaml","runOutcome":"failure","failure":{"code":"NotVisible","message":"no match","selector":null,"suggestions":[],"visibleCount":3}}"#,
            "\n",
        );
        let reports = parse_report_lines(stdout).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0].flow,
            "scripts/release/stress-corpus/launch-and-capture.yaml"
        );
        assert_eq!(reports[0].outcome, "success");
        assert_eq!(
            reports[1].flow,
            "scripts/release/stress-corpus/screenshot-twice.yaml"
        );
        assert_eq!(reports[1].outcome, "failure");
        assert_eq!(reports[1].raw["failure"]["code"], "NotVisible");
    }

    #[test]
    fn rejects_a_non_json_stdout_line() {
        let stdout = concat!(
            r#"{"flow":"a.yaml","runOutcome":"success","warnings":[],"steps":[]}"#,
            "\n\n",
            "kevy: AOF 3 entries replayed\n",
        );
        let err = parse_report_lines(stdout).unwrap_err();
        assert!(
            matches!(&err, ReportError::NotJson { line } if line == "kevy: AOF 3 entries replayed")
        );
        assert!(err.to_string().contains("kevy: AOF 3 entries replayed"));
    }

    #[test]
    fn readiness_argv_pins_the_gate_command() {
        let node = NodeSpec {
            name: "mini".to_string(),
            host: "mini".to_string(),
            repo: "/Users/doracawl/workspace/goliajp/smix".to_string(),
            devices: vec!["sim-smix-001".to_string()],
        };
        assert_eq!(
            readiness_argv(&node),
            vec![
                "-o".to_string(),
                "BatchMode=yes".to_string(),
                "mini".to_string(),
                "cd '/Users/doracawl/workspace/goliajp/smix' && \
                 test -f target/.smix-fed-stamp && \
                 test -x target/release/smix && \
                 [ -z \"$(find crates -name '*.rs' -newer target/.smix-fed-stamp)\" ]"
                    .to_string(),
            ]
        );
    }

    /// Single-node e2e over a live node: parse_nodes → expand_slots →
    /// assign_flows → readiness gate → remote_argv → run_ssh → JSON
    /// report lines. Driven by scripts/dev/
    /// v2.12-c3-federation-single-node-e2e.sh, which owns the sync,
    /// rebuild, device prep and teardown around it.
    #[test]
    #[ignore]
    fn federation_e2e_single_node_runs_flows_on_mini() {
        let nodes_path = std::env::var("SMIX_FED_E2E_NODES")
            .expect("SMIX_FED_E2E_NODES unset — this test is driven by the C3 e2e script");
        let flows_env = std::env::var("SMIX_FED_E2E_FLOWS")
            .expect("SMIX_FED_E2E_FLOWS unset — this test is driven by the C3 e2e script");
        let flows: Vec<String> = flows_env.split(',').map(str::to_string).collect();
        assert!(!flows.is_empty());

        let yaml = std::fs::read_to_string(&nodes_path).unwrap();
        let nodes = parse_nodes(&yaml).unwrap();
        let slots = expand_slots(&nodes);
        assert_eq!(slots.len(), 1, "single-node e2e expects exactly one slot");
        let assignments = assign_flows(flows.len(), &slots);
        assert_eq!(assignments[0].flows, (0..flows.len()).collect::<Vec<_>>());
        let node = &nodes[assignments[0].node];
        let device_ref = &assignments[0].device_ref;

        let gate = run_ssh(&readiness_argv(node)).unwrap();
        assert_eq!(
            gate.exit, 0,
            "readiness gate failed — node is stale\nstderr: {}",
            gate.stderr
        );

        let out = run_ssh(&remote_argv(node, &flows, device_ref, &[])).unwrap();
        assert!(
            !is_transport_failure(out.exit),
            "ssh transport failure\nstderr: {}",
            out.stderr
        );
        assert_eq!(out.exit, 0, "remote run failed\nstderr: {}", out.stderr);

        let reports = parse_report_lines(&out.stdout).unwrap();
        assert_eq!(reports.len(), flows.len());
        for (report, flow) in reports.iter().zip(&flows) {
            assert_eq!(&report.flow, flow);
            assert_eq!(report.outcome, "success", "flow {flow} failed: {}", report.raw);
        }
    }

    #[test]
    fn rejects_duplicate_node_names() {
        let yaml = "\
nodes:
  - name: mini
    host: mini-a
    repo: /a
    devices: [sim-1]
  - name: mini
    host: mini-b
    repo: /b
    devices: [sim-2]
";
        let err = parse_nodes(yaml).unwrap_err();
        assert!(matches!(&err, NodesError::DuplicateName { name } if name == "mini"));
    }
}
