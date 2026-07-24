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
