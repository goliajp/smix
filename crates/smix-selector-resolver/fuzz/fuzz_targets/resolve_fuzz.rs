#![no_main]
//! Fuzz selector-resolver end-to-end. Split input into tree JSON + selector
//! JSON, parse, call resolve_selector + resolve_selector_all. Should never
//! panic, regardless of selector / tree shape.

use libfuzzer_sys::fuzz_target;
use smix_screen::A11yNode;
use smix_selector::Selector;
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

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

    let _ = resolve_selector(&tree, &sel);
    let all = resolve_selector_all(&tree, &sel);
    let _ = all.len();
});
