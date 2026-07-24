//! Dev-only thin shell driving `propose_and_amend` with the real local
//! `claude` CLI (`AiTierConfig::default()`). Reads a failed flow + its bundle,
//! writes the amended flow yaml.
//!
//!   cargo run -p smix-authoring-propose --example propose_amend -- <flow> <bundle> <out>
//!
//! Exit 0 on a written amend; exit 1 (with the `AmendError` on stderr) otherwise.

use std::path::Path;

use smix_ai_tier::AiTierConfig;
use smix_authoring_propose::propose_and_amend;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: propose_amend <flow> <bundle> <out>");
        std::process::exit(2);
    }
    let flow = Path::new(&args[1]);
    let bundle = Path::new(&args[2]);
    let out = Path::new(&args[3]);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let cfg = AiTierConfig::default();
    match rt.block_on(propose_and_amend(flow, bundle, &cfg)) {
        Ok(yaml) => {
            std::fs::write(out, yaml).expect("write amended flow");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    }
}
