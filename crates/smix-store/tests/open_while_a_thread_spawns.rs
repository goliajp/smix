//! A process that starts children on one thread while another opens and
//! closes the store must not see the store refused as already open.
//!
//! A child holds copies of the parent's descriptors from fork until exec
//! closes them, one at a time; between the two closes the store lock is
//! free and the engine's directory lock is not.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn opening_beside_a_spawning_thread_is_never_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let spawners: Vec<_> = (0..4)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::process::Command::new("true").status();
                }
            })
        })
        .collect();
    let mut refused = None;
    for i in 0..3000 {
        if let Err(e) = smix_store::Store::open(dir.path()) {
            refused = Some(format!("open {i}: {e}"));
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    for s in spawners {
        s.join().unwrap();
    }
    assert_eq!(refused, None);
}
