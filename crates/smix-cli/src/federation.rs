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
    capture("ssh", argv)
}

fn capture(program: &str, argv: &[String]) -> std::io::Result<RemoteOutput> {
    let out = std::process::Command::new(program).args(argv).output()?;
    Ok(RemoteOutput {
        exit: out.status.code().map_or(1, |c| c.clamp(0, 255) as u8),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Where a federation run's `--debug-output` artifacts land on each
/// node, relative to the node's repo (bare-safe: it rides the remote
/// argv passthrough verbatim, unquoted).
pub const FED_ARTIFACT_DIR: &str = ".smix/fed-c4-artifacts";

/// Build the argv for the artifact-recovery `rsync` (the `"rsync"` word
/// itself excluded, the same convention as `readiness_argv`): archive
/// pull of the node's remote artifact dir into a per-node local subdir.
/// Trailing slashes on both sides give directory-content semantics; the
/// local side is keyed by the node name, so same-basename artifacts
/// from different nodes never collide — the wrapper layer solves the
/// collision, the bundle content stays untouched.
#[must_use]
pub fn artifact_pull_argv(node: &NodeSpec, remote_dir: &str, local_dir: &str) -> Vec<String> {
    vec![
        "-a".to_string(),
        format!(
            "{}:{}",
            node.host,
            shell_quote(&format!("{}/{}/", node.repo, remote_dir))
        ),
        format!("{}/{}/", local_dir, node.name),
    ]
}

/// Spawn `rsync` with the given argv and capture everything — the same
/// shape as `run_ssh`: no retry, no timeout, a transfer failure
/// surfaces as its own non-zero exit.
pub fn run_rsync(argv: &[String]) -> std::io::Result<RemoteOutput> {
    capture("rsync", argv)
}

/// One node's complete outcome: its exit code and the per-flow reports
/// parsed off its stdout. A transport-lost node (exit 255) has no
/// usable stdout and carries an empty `reports`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeResult {
    pub name: String,
    pub exit: u8,
    pub reports: Vec<FlowReport>,
}

/// One node inside the merged CI report: a pure wrapper around the
/// node's report-line leaves, which stay verbatim `serde_json::Value`s.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MergedNode {
    pub node: String,
    pub exit: u8,
    pub flows: Vec<serde_json::Value>,
}

/// The merged CI report for a federation batch: every node's leaves
/// wrapped under its name/exit, plus the worst-of-nodes aggregate.
/// Serialization is plain `serde_json::to_string` — one JSON document
/// is the CI consumption surface, no bespoke emitter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedReport {
    pub nodes: Vec<MergedNode>,
    pub aggregate_exit: u8,
}

/// Merge per-node results into one report. The aggregate is
/// `parallel::aggregate_exit` verbatim — one worst-of semantic,
/// maintained in one place; the 255 transport sentinel wins by max.
/// The flow leaves are the remote report lines untouched.
#[must_use]
pub fn merge_reports(results: &[NodeResult]) -> MergedReport {
    let exits: Vec<u8> = results.iter().map(|r| r.exit).collect();
    MergedReport {
        nodes: results
            .iter()
            .map(|r| MergedNode {
                node: r.name.clone(),
                exit: r.exit,
                flows: r.reports.iter().map(|f| f.raw.clone()).collect(),
            })
            .collect(),
        aggregate_exit: crate::parallel::aggregate_exit(&exits),
    }
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
    fn artifact_pull_argv_pins_the_rsync_command() {
        let node = NodeSpec {
            name: "mini".to_string(),
            host: "mini".to_string(),
            repo: "/Users/doracawl/workspace/goliajp/smix".to_string(),
            devices: vec!["sim-smix-001".to_string()],
        };
        assert_eq!(
            artifact_pull_argv(&node, FED_ARTIFACT_DIR, "/tmp/pull"),
            vec![
                "-a".to_string(),
                "mini:'/Users/doracawl/workspace/goliajp/smix/.smix/fed-c4-artifacts/'"
                    .to_string(),
                "/tmp/pull/mini/".to_string(),
            ]
        );
    }

    /// Two-node e2e over live nodes (studio via `ssh localhost` + mini):
    /// parse_nodes → expand_slots → assign_flows → per-node readiness
    /// gate → remote run with `--debug-output` passthrough → JSON report
    /// lines → merge_reports → per-node artifact rsync recovery. Driven
    /// by scripts/dev/v2.12-c4-federation-two-node-e2e.sh, which owns
    /// the self-authorization, sync, rebuild, device prep and teardown
    /// around it.
    #[test]
    #[ignore]
    fn federation_e2e_two_nodes_merge_reports_and_recover_artifacts() {
        let nodes_path = std::env::var("SMIX_FED_E2E_NODES")
            .expect("SMIX_FED_E2E_NODES unset — this test is driven by the C4 e2e script");
        let flows_env = std::env::var("SMIX_FED_E2E_FLOWS")
            .expect("SMIX_FED_E2E_FLOWS unset — this test is driven by the C4 e2e script");
        let pull_dir = std::env::var("SMIX_FED_E2E_PULL_DIR")
            .expect("SMIX_FED_E2E_PULL_DIR unset — this test is driven by the C4 e2e script");
        let ports_env = std::env::var("SMIX_FED_E2E_RUNNER_PORTS").unwrap_or_default();
        let ports: std::collections::HashMap<String, String> = ports_env
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                let (name, port) = pair
                    .split_once('=')
                    .expect("SMIX_FED_E2E_RUNNER_PORTS entry is name=port");
                (name.to_string(), port.to_string())
            })
            .collect();
        let flows: Vec<String> = flows_env.split(',').map(str::to_string).collect();
        assert_eq!(flows.len(), 2, "two-node e2e expects exactly two flows");

        let yaml = std::fs::read_to_string(&nodes_path).unwrap();
        let nodes = parse_nodes(&yaml).unwrap();
        assert_eq!(nodes.len(), 2, "two-node e2e expects exactly two nodes");
        let slots = expand_slots(&nodes);
        assert_eq!(slots.len(), 2, "two-node e2e expects exactly two slots");
        let assignments = assign_flows(flows.len(), &slots);

        let mut results = Vec::new();
        for assignment in &assignments {
            assert_eq!(
                assignment.flows.len(),
                1,
                "each slot carries exactly one flow"
            );
            let node = &nodes[assignment.node];
            let node_flows: Vec<String> =
                assignment.flows.iter().map(|&i| flows[i].clone()).collect();

            let gate = run_ssh(&readiness_argv(node)).unwrap();
            assert_eq!(
                gate.exit, 0,
                "readiness gate failed on {} — node is stale\nstderr: {}",
                node.name, gate.stderr
            );

            let mut passthrough =
                vec!["--debug-output".to_string(), FED_ARTIFACT_DIR.to_string()];
            if let Some(port) = ports.get(&node.name) {
                passthrough.push("--runner-port".to_string());
                passthrough.push(port.clone());
            }
            let out = run_ssh(&remote_argv(
                node,
                &node_flows,
                &assignment.device_ref,
                &passthrough,
            ))
            .unwrap();
            assert!(
                !is_transport_failure(out.exit),
                "ssh transport failure on {}\nstderr: {}",
                node.name,
                out.stderr
            );
            assert_eq!(
                out.exit, 0,
                "remote run failed on {}\nstderr: {}",
                node.name, out.stderr
            );

            let reports = parse_report_lines(&out.stdout).unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(&reports[0].flow, &node_flows[0]);
            assert_eq!(
                reports[0].outcome, "success",
                "flow {} failed on {}: {}",
                node_flows[0], node.name, reports[0].raw
            );
            results.push(NodeResult {
                name: node.name.clone(),
                exit: out.exit,
                reports,
            });
        }

        let merged = merge_reports(&results);
        assert_eq!(merged.nodes.len(), 2);
        assert_eq!(merged.aggregate_exit, 0);
        let doc = serde_json::to_string(&merged).unwrap();
        for node in &nodes {
            assert!(
                doc.contains(&node.name),
                "merged report is missing node {}",
                node.name
            );
        }

        for node in &nodes {
            let pull = run_rsync(&artifact_pull_argv(node, FED_ARTIFACT_DIR, &pull_dir)).unwrap();
            assert_eq!(
                pull.exit, 0,
                "artifact rsync failed for {}\nstderr: {}",
                node.name, pull.stderr
            );
            let summary = std::path::Path::new(&pull_dir)
                .join(&node.name)
                .join("run-summary.json");
            assert!(
                summary.is_file(),
                "recovered artifact missing: {}",
                summary.display()
            );
        }
    }

    #[test]
    fn merges_two_nodes_into_one_wrapped_report_pinning_the_json() {
        let leaf_a = r#"{"flow":"scripts/release/stress-corpus/launch-and-capture.yaml","runOutcome":"success","warnings":[],"steps":[]}"#;
        let leaf_b = r#"{"flow":"scripts/release/stress-corpus/screenshot-twice.yaml","runOutcome":"success","warnings":[],"steps":[]}"#;
        let results = vec![
            NodeResult {
                name: "a".to_string(),
                exit: 0,
                reports: parse_report_lines(leaf_a).unwrap(),
            },
            NodeResult {
                name: "b".to_string(),
                exit: 0,
                reports: parse_report_lines(leaf_b).unwrap(),
            },
        ];
        let merged = merge_reports(&results);
        // The wrapper carries node/exit/flows + aggregateExit and nothing
        // else; the leaves are the report lines verbatim (serde_json's
        // Value serializes object keys alphabetically — the leaf content
        // is untouched, only that canonical key order applies).
        assert_eq!(
            serde_json::to_string(&merged).unwrap(),
            concat!(
                r#"{"nodes":["#,
                r#"{"node":"a","exit":0,"flows":[{"flow":"scripts/release/stress-corpus/launch-and-capture.yaml","runOutcome":"success","steps":[],"warnings":[]}]},"#,
                r#"{"node":"b","exit":0,"flows":[{"flow":"scripts/release/stress-corpus/screenshot-twice.yaml","runOutcome":"success","steps":[],"warnings":[]}]}"#,
                r#"],"aggregateExit":0}"#,
            )
        );
    }

    #[test]
    fn transport_255_node_merges_empty_and_wins_the_aggregate() {
        let leaf = r#"{"flow":"a.yaml","runOutcome":"success","warnings":[],"steps":[]}"#;
        let results = vec![
            NodeResult {
                name: "a".to_string(),
                exit: 0,
                reports: parse_report_lines(leaf).unwrap(),
            },
            // Transport loss: ssh exited 255, its stdout is not a report
            // channel — the node merges in with no flows, never dropped.
            NodeResult {
                name: "b".to_string(),
                exit: SSH_TRANSPORT_EXIT,
                reports: vec![],
            },
        ];
        let merged = merge_reports(&results);
        assert_eq!(merged.nodes.len(), 2);
        assert_eq!(merged.nodes[1].node, "b");
        assert_eq!(merged.nodes[1].flows, Vec::<serde_json::Value>::new());
        assert_eq!(merged.aggregate_exit, 255);
        assert!(is_transport_failure(merged.aggregate_exit));
    }

    #[test]
    fn a_node_with_all_failed_flows_keeps_failure_leaves_verbatim() {
        let stdout = concat!(
            r#"{"flow":"a.yaml","runOutcome":"failure","failure":{"code":"NotVisible","message":"no match","selector":null,"suggestions":[],"visibleCount":3}}"#,
            "\n",
            r#"{"flow":"b.yaml","runOutcome":"failure","failure":{"code":"Timeout","message":"gave up","selector":null,"suggestions":[],"visibleCount":0}}"#,
            "\n",
        );
        let results = vec![NodeResult {
            name: "mini".to_string(),
            exit: 3,
            reports: parse_report_lines(stdout).unwrap(),
        }];
        let merged = merge_reports(&results);
        assert_eq!(merged.nodes[0].flows.len(), 2);
        assert_eq!(merged.nodes[0].flows[0]["failure"]["code"], "NotVisible");
        assert_eq!(merged.nodes[0].flows[1]["failure"]["code"], "Timeout");
        assert_eq!(merged.aggregate_exit, 3);
    }

    #[test]
    fn empty_inputs_merge_to_exit_zero() {
        // No nodes at all: the same empty-set semantic as aggregate_exit.
        let merged = merge_reports(&[]);
        assert_eq!(merged.nodes, Vec::<MergedNode>::new());
        assert_eq!(merged.aggregate_exit, 0);
        // A node that ran nothing (more slots than flows): flows stay
        // empty and the aggregate stays green.
        let merged = merge_reports(&[NodeResult {
            name: "idle".to_string(),
            exit: 0,
            reports: vec![],
        }]);
        assert_eq!(merged.nodes.len(), 1);
        assert_eq!(merged.nodes[0].flows, Vec::<serde_json::Value>::new());
        assert_eq!(merged.aggregate_exit, 0);
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
