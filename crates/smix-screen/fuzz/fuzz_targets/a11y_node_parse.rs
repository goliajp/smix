#![no_main]
//! Fuzz the A11yNode JSON parse path. Runner returns untrusted JSON for
//! every `/tree` HTTP call — parse must reject malformed input without
//! panicking. Also walks the parsed tree (visibility filter, summary
//! collection) since those are downstream of parse on the same byte path.

use libfuzzer_sys::fuzz_target;
use smix_screen::{A11yNode, collect_visible_summaries, is_visible_enough, summarize_node};

fuzz_target!(|data: &[u8]| {
    let Ok(tree) = serde_json::from_slice::<A11yNode>(data) else {
        return;
    };
    let _ = is_visible_enough(&tree, &tree);
    let _ = summarize_node(&tree);
    let _ = collect_visible_summaries(&tree);
});
