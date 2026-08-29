#![no_main]
//! Fuzz the Pattern::compile + match_text_compiled path. The regex crate
//! has its own well-tested fuzz coverage; what we want to surface here is
//! "any UTF-8 string fed as a regex pattern, when wrapped in our
//! auto-/i-injecting Pattern::Regex, must either return Err or compile
//! into a CompiledPattern that doesn't panic on match against an
//! arbitrary node payload".

use libfuzzer_sys::fuzz_target;
use smix_screen::{A11yNode, Rect};
use smix_selector::Pattern;

fn synth_node(label_seed: &str) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: Some(label_seed.to_string()),
        label: Some(label_seed.to_string()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let pat = Pattern::Regex {
        regex: s.to_string(),
        flags: "i".to_string(),
    };
    let Ok(compiled) = pat.compile() else {
        return;
    };
    let node = synth_node(s);
    let _ = smix_selector::match_text_compiled(&node, &compiled);
});
