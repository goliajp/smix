//! v5.1 c3 S1 — issued-action 账本单元测。
//!
//! Capsule 软胶囊 G angle ground truth proxy 由两件事拼起:UITest runner 的
//! EventRecorder 抓 1018 焦点变化(swift 端,已 wired),SDK 端记每次 issued
//! act 的 timestamp + 类型。本测试钉 SDK 端账本契约:LRU 1024 + record/clear
//! 不漏不重复 + 三个便捷入口生成对应 `IssuedKind`。

use smix_sdk::{IssuedAction, IssuedKind, IssuedLedger};

#[test]
fn case1_new_ledger_is_empty() {
    let l = IssuedLedger::new();
    assert!(l.get_all().is_empty());
}

#[test]
fn case2_record_one_then_get() {
    let l = IssuedLedger::new();
    l.record(IssuedAction {
        kind: IssuedKind::Tap,
        timestamp_ms: 1000.0,
        target_hint: Some("input-email".into()),
    });
    let snap = l.get_all();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].kind, IssuedKind::Tap);
    assert_eq!(snap[0].timestamp_ms, 1000.0);
    assert_eq!(snap[0].target_hint.as_deref(), Some("input-email"));
}

#[test]
fn case3_lru_drops_oldest_past_cap_1024() {
    let l = IssuedLedger::new();
    for i in 1..=1025_u32 {
        l.record(IssuedAction {
            kind: IssuedKind::Tap,
            timestamp_ms: f64::from(i),
            target_hint: None,
        });
    }
    let snap = l.get_all();
    assert_eq!(snap.len(), 1024);
    assert_eq!(snap.first().map(|a| a.timestamp_ms), Some(2.0));
    assert_eq!(snap.last().map(|a| a.timestamp_ms), Some(1025.0));
}

#[test]
fn case4_clear_empties() {
    let l = IssuedLedger::new();
    l.record(IssuedAction {
        kind: IssuedKind::Tap,
        timestamp_ms: 1.0,
        target_hint: None,
    });
    l.clear();
    assert!(l.get_all().is_empty());
}

#[test]
fn case5_convenience_constructors() {
    let l = IssuedLedger::new();
    l.record_tap(100.0, Some("input-email".into()));
    l.record_fill(200.0, Some("input-password".into()));
    l.record_tap_at_coord(300.0, 0.5, 0.5);
    let snap = l.get_all();
    assert_eq!(snap.len(), 3);
    assert_eq!(snap[0].kind, IssuedKind::Tap);
    assert_eq!(snap[0].timestamp_ms, 100.0);
    assert_eq!(snap[0].target_hint.as_deref(), Some("input-email"));
    assert_eq!(snap[1].kind, IssuedKind::Fill);
    assert_eq!(snap[1].timestamp_ms, 200.0);
    assert_eq!(snap[1].target_hint.as_deref(), Some("input-password"));
    assert_eq!(snap[2].kind, IssuedKind::TapAtCoord { nx: 0.5, ny: 0.5 });
    assert_eq!(snap[2].timestamp_ms, 300.0);
    assert_eq!(snap[2].target_hint, None);
}

#[test]
fn case6_fixture_action_anchor() {
    let l = IssuedLedger::new();
    l.record_fixture_action(400.0, "v2-share-present".into());
    let snap = l.get_all();
    assert_eq!(snap.len(), 1);
    assert_eq!(
        snap[0].kind,
        IssuedKind::FixtureAction("v2-share-present".into())
    );
    assert_eq!(snap[0].timestamp_ms, 400.0);
    assert_eq!(snap[0].target_hint.as_deref(), Some("v2-share-present"));
}
