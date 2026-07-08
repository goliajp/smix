#![no_main]
//! Fuzz host-coord-resolver end-to-end. Split input bytes into half-tree
//! / half-selector JSON, parse, call resolve_to_norm_coord, validate no
//! panic + result is either Ok((nx,ny)) in (0,1) or a known Err variant.

use libfuzzer_sys::fuzz_target;
use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_screen::A11yNode;
use smix_selector::Selector;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let split = (data[0] as usize % (data.len() - 1)).max(1);
    let tree_bytes = &data[..split];
    let sel_bytes = &data[split..];

    let Ok(tree) = serde_json::from_slice::<A11yNode>(tree_bytes) else {
        return;
    };
    let Ok(sel) = serde_json::from_slice::<Selector>(sel_bytes) else {
        return;
    };

    match resolve_to_norm_coord(&tree, &sel) {
        Ok((nx, ny)) => {
            // Should be in (0, 1) on success.
            assert!(nx > 0.0 && nx < 1.0, "nx out of range: {}", nx);
            assert!(ny > 0.0 && ny < 1.0, "ny out of range: {}", ny);
        }
        Err(
            HostResolveError::NotFound
            | HostResolveError::EmptyMatchedFrame
            | HostResolveError::UnknownAppFrame
            | HostResolveError::CentroidOutOfFrame { .. },
        ) => {}
    }
});
