//! Rust perf baseline. Measures `resolve_selector` round-trip
//! against a fixed tree+selector, mirroring the Swift / Kotlin / TS
//! `Smix.launchApp + App.tap` benchmarks.
//!
//! Usage: `cargo run -p smix-core-conformance --bin perf-baseline --release`
//! Output: human-readable baseline + regression-gate thresholds to stdout.

use std::time::Instant;

fn main() {
    let iterations = 1_000;
    let warmup = 100;

    // Fixed tree: 1 button "btn-x" at logical (140, 220).
    let tree_json = r#"{
        "rawType":"other",
        "bounds":{"x":0,"y":0,"w":393,"h":852},
        "enabled":true,"selected":false,"hasFocus":false,"visible":true,
        "children":[{
            "rawType":"button","role":"button",
            "identifier":"btn-x","label":"Tap me",
            "bounds":{"x":100,"y":200,"w":80,"h":40},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "children":[]
        }]
    }"#;
    let selector_json = r#"{"id":"btn-x"}"#;

    // Warmup (cargo bench-style JIT settle equivalent for branch
    // predictor / icache).
    for _ in 0..warmup {
        let _ = smix_ffi::resolve_selector(tree_json.to_string(), selector_json.to_string());
    }

    // Measure
    let mut samples: Vec<f64> = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = smix_ffi::resolve_selector(tree_json.to_string(), selector_json.to_string())
            .expect("resolve_selector should succeed");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0; // ms
        samples.push(elapsed);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let median = samples[samples.len() / 2];
    let p99 = samples[(samples.len() as f64 * 0.99) as usize];
    let min_v = samples[0];
    let max_v = samples[samples.len() - 1];
    let avg = samples.iter().sum::<f64>() / samples.len() as f64;

    println!("# SmixSDK Rust perf baseline");
    println!(
        "# Date: {}",
        std::env::var("PERF_DATE").unwrap_or_else(|_| "(set PERF_DATE env)".into())
    );
    println!("# Operation: smix_ffi::resolve_selector (tree → ids)");
    println!("# Backend: in-process FFI fn (no IPC / sim)");
    println!("# Iterations: {} (after {} warmup)", iterations, warmup);
    println!();
    println!("min:    {:.3} ms", min_v);
    println!("avg:    {:.3} ms", avg);
    println!("median: {:.3} ms", median);
    println!("p99:    {:.3} ms", p99);
    println!("max:    {:.3} ms", max_v);
    println!();
    println!("# Regression gate:");
    println!("#   soft fail if median > {:.3} ms (1.5x)", median * 1.5);
    println!("#   hard fail if median > {:.3} ms (3x)", median * 3.0);
}
