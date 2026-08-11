//! `smix init` has to leave the next command able to run.
//!
//! Device records moved to the machine in 4.0. Creating the checkout's
//! `.smix/` used to be a side effect of writing the registry into it, and
//! when the registry stopped going there, nothing created it. `init` went
//! on reporting success — it had registered the device, which is what it
//! says it does — and the very next command answered
//!
//!     no .smix/ workspace found upward from …
//!
//! It was invisible on the machine it was written on, where every tree
//! already had a `.smix/` from before the move. CI found it on a clean
//! runner: `portable corpus tier` initialised, then failed at `runner up`.
//!
//! So the check is not "did init write a registry" but "can the next
//! command run" — the two came apart, and only the second is what init
//! is for.

use std::process::Command;

fn smix() -> &'static str {
    env!("CARGO_BIN_EXE_smix")
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "smix-init-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A tree that has never seen smix, and a machine that has not either.
///
/// No device is booted and none needs to be: `init` fails before it
/// would touch one when the UDID names nothing, and what is checked here
/// is the directory it must leave behind either way — the run below is
/// against a device id `simctl` will not know, so the assertion is on
/// the workspace and not on the registration.
#[test]
fn init_creates_the_checkout_workspace_the_next_command_looks_for() {
    let tree = tmp("tree");
    let machine = tmp("machine");

    // Whatever this returns, the question is what it leaves behind.
    let _ = Command::new(smix())
        .args(["init", "--device", "FFFFFFFF-1111-2222-3333-999999999999"])
        .current_dir(&tree)
        .env("SMIX_MACHINE_DIR", &machine)
        .output()
        .expect("run smix init");

    let workspace = tree.join(".smix");
    let made_one = workspace.is_dir();

    std::fs::remove_dir_all(&tree).ok();
    std::fs::remove_dir_all(&machine).ok();

    assert!(
        made_one,
        "`smix init` did not leave a .smix/ in the tree it ran in. That \
         directory is what `runner up` walks up to find, and what a run's \
         traces and runner state go into — checkout-scoped facts that the \
         move to machine-scoped device records did not touch. Without it, \
         init reports success and the next command cannot run."
    );
}
