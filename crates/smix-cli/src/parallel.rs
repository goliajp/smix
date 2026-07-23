//! Sharding flows across sims for `smix run --parallel N`.
//!
//! The assignment is a pure function, kept apart from the concurrent
//! runner orchestration so it can be pinned by unit tests without a
//! device. `--parallel 1` (one sim) must produce the exact single-sim
//! order the sequential path has always run, so that the default stays
//! byte-identical.

/// Assign `flow_count` flows across `device_count` sims, round-robin.
///
/// Returns one bucket per sim, each holding the flow indices that sim
/// runs, in ascending order. Flow `i` goes to sim `i % device_count`.
/// Round-robin balances the flow *count* per sim; it does not model
/// per-flow duration (unknown before running), so it is the honest
/// deterministic choice over a "least-loaded" guess.
///
/// - `device_count == 1` → one bucket holding `0..flow_count` in order,
///   which is the sequential single-sim path unchanged.
/// - `device_count == 0` → empty (the caller must have at least one
///   sim; this returns nothing rather than panicking).
/// - `flow_count < device_count` → the surplus sims get empty buckets.
#[must_use]
pub fn shard_flows(flow_count: usize, device_count: usize) -> Vec<Vec<usize>> {
    if device_count == 0 {
        return Vec::new();
    }
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); device_count];
    for flow in 0..flow_count {
        buckets[flow % device_count].push(flow);
    }
    buckets
}

/// How many sims to actually use: the requested parallelism, capped by
/// the sims available. `--parallel 3` with two devices uses two.
#[must_use]
pub fn effective_sim_count(requested_parallel: usize, devices_available: usize) -> usize {
    requested_parallel.clamp(1, devices_available.max(1))
}

/// The batch exit code: the worst of the shards.
///
/// A failed shard fails the whole run; nothing is swallowed. An empty
/// set (no shard ran) is a success — there was nothing to fail.
#[must_use]
pub fn aggregate_exit(shard_codes: &[u8]) -> u8 {
    shard_codes.iter().copied().max().unwrap_or(0)
}

/// Build the argv for one shard's child `smix run` — the flows assigned
/// to a sim, pinned to that sim's UDID, with the parent's pass-through
/// flags appended verbatim.
///
/// A shard is a plain single-sim `smix run`, so it reuses the
/// battle-tested sequential path unchanged rather than re-implementing
/// it concurrently. `--parallel`/`--also-device` are deliberately NOT
/// forwarded — the child runs one sim.
#[must_use]
pub fn child_argv(shard_flows: &[String], udid: &str, passthrough: &[String]) -> Vec<String> {
    let mut argv = vec!["run".to_string()];
    argv.extend(shard_flows.iter().cloned());
    argv.push("--device".to_string());
    argv.push(udid.to_string());
    argv.extend(passthrough.iter().cloned());
    argv
}

/// Run each shard as a concurrent child `smix run`, then aggregate.
///
/// Every shard is spawned first (so they run at once), then joined. A
/// shard with no flows is skipped. A spawn failure counts as a failed
/// shard (exit 1) rather than being silently dropped. Config/env-sourced
/// behaviour switches are inherited by the children from the same
/// `.smix/config.yaml` + env, so only CLI-sourced flags are forwarded.
#[must_use]
pub fn run_parallel(
    exe: &std::path::Path,
    shards: &[(String, Vec<String>)],
    passthrough: &[String],
) -> u8 {
    let mut running = Vec::new();
    for (udid, flows) in shards {
        if flows.is_empty() {
            continue;
        }
        let argv = child_argv(flows, udid, passthrough);
        eprintln!("smix run --parallel: sim {udid} ← {} flow(s)", flows.len());
        match std::process::Command::new(exe).args(&argv).spawn() {
            Ok(child) => running.push((udid.clone(), Some(child))),
            Err(e) => {
                eprintln!("smix run --parallel: sim {udid} failed to spawn: {e}");
                running.push((udid.clone(), None));
            }
        }
    }
    let mut codes = Vec::new();
    for (udid, child) in running {
        let code = match child {
            None => 1,
            Some(mut c) => c
                .wait()
                .ok()
                .and_then(|s| s.code())
                .map_or(1, |c| c.clamp(0, 255) as u8),
        };
        eprintln!("smix run --parallel: sim {udid} exited {code}");
        codes.push(code);
    }
    aggregate_exit(&codes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sim_keeps_the_sequential_order() {
        // The default path: every flow on one sim, in order — the
        // single-sim behaviour that must stay byte-identical.
        assert_eq!(shard_flows(3, 1), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn two_sims_round_robin() {
        assert_eq!(shard_flows(3, 2), vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn more_sims_than_flows_leaves_idle_sims_with_empty_buckets() {
        assert_eq!(shard_flows(2, 3), vec![vec![0], vec![1], vec![]]);
    }

    #[test]
    fn every_flow_is_assigned_exactly_once() {
        let buckets = shard_flows(7, 3);
        let mut all: Vec<usize> = buckets.into_iter().flatten().collect();
        all.sort_unstable();
        assert_eq!(all, (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn no_sims_assigns_nothing_rather_than_panicking() {
        assert_eq!(shard_flows(5, 0), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn parallelism_is_capped_by_available_sims() {
        assert_eq!(effective_sim_count(3, 2), 2); // asked 3, have 2
        assert_eq!(effective_sim_count(1, 4), 1); // default stays 1
        assert_eq!(effective_sim_count(4, 4), 4);
        assert_eq!(effective_sim_count(2, 0), 1); // never zero
    }

    #[test]
    fn a_failed_shard_fails_the_whole_run() {
        assert_eq!(aggregate_exit(&[0, 0, 0]), 0);
        assert_eq!(aggregate_exit(&[0, 2, 0]), 2); // one failure loses
        assert_eq!(aggregate_exit(&[]), 0); // nothing ran, nothing failed
    }

    #[test]
    fn child_argv_pins_the_sim_and_forwards_passthrough() {
        let argv = child_argv(
            &["a.yaml".into(), "b.yaml".into()],
            "UDID-1",
            &["--bundle-id".into(), "com.x".into(), "--no-launch".into()],
        );
        assert_eq!(
            argv,
            vec![
                "run",
                "a.yaml",
                "b.yaml",
                "--device",
                "UDID-1",
                "--bundle-id",
                "com.x",
                "--no-launch"
            ]
        );
        // --parallel / --also-device must not recurse into the child.
        assert!(
            !argv
                .iter()
                .any(|a| a == "--parallel" || a == "--also-device")
        );
    }
}
