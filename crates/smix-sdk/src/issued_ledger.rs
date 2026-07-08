//! v5.1 c3 — SDK 内部 issued-action 账本(Capsule 软胶囊 G angle 主线)。
//!
//! Capsule 软胶囊检测外来 user 干预的对账逻辑:
//!   UITest runner EventRecorder 抓 1018 焦点变化 events ── ground truth
//!     ∖
//!   SDK 端 issued-action 账本(每次 `app.tap` / `app.fill` / `app.tap_at_coord`
//!     落账 timestamp + 类型 + selector hint)── SDK 自发的动作
//!   = 外来 user 干预的焦点变化
//!
//! 容量上限 [`LEDGER_CAP`] = 1024,push 后超容量 pop_front(LRU)。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// LRU 容量上限。每个 SDK act 入口一条记录,1024 足够覆盖典型 capsule
/// session(start_record → 大量 tap/fill → stop_record 几分钟内的全量)。
pub const LEDGER_CAP: usize = 1024;

/// SDK 发起的 act 类型。`tap_at_coord` 携带归一化坐标(0..1),`Tap` / `Fill`
/// 走 selector 路径,`target_hint` 在 [`IssuedAction`] 上携带 selector 描述。
#[derive(Clone, Debug, PartialEq)]
pub enum IssuedKind {
    Tap,
    Fill,
    TapAtCoord {
        nx: f64,
        ny: f64,
    },
    SwipeAtCoord {
        from: (f64, f64),
        to: (f64, f64),
    },
    /// v5.2 c1 — capsule recording 启动锚点。`start_capsule_recording` 调
    /// `driver.start_record()` 触发 swift `EventRecorder.installSwizzle +
    /// start` — fixture lifecycle settle 期(swizzle 安装后 firstResponder
    /// 重置等)抓到的 1018 焦点变化在物理上是 SDK 自发的 capsule start 引起,
    /// 应被 reconcile attribute 到本 act,不被算 user 干预。
    CapsuleStart,
    /// v5.9 c2 — fixture-side action 锚点。 fixture-owned UIKit modal present
    /// (UIActivityViewController / UIDocumentPickerViewController /
    /// SpringBoard permission alert) 触发 `kAXFirstResponderChangedNotification`
    /// 1018 events, 但 SDK driver 没直接 dispatch — fixture-side delegate
    /// 走 UIKit native API。 selftest seg 在调 fixture trigger button 之前
    /// 调 `App::mark_fixture_action(action_id)` 把 expected phantom focus
    /// change 钉到 ledger, reconcile window (3000ms) 内 attribute 到本 mark
    /// 不算 user 干预。 升级 v5.7 c2 UNATTR_MAX=1 cushion 到 architectural fix。
    FixtureAction(String),
}

/// 一条 SDK 发起的 act 记录。
#[derive(Clone, Debug, PartialEq)]
pub struct IssuedAction {
    pub kind: IssuedKind,
    /// 毫秒级 Unix epoch(`chrono::Utc::now().timestamp_millis() as f64`)。
    pub timestamp_ms: f64,
    /// Selector 描述,典型是 `selector.id` / `selector.text` /
    /// `selector.label`。`tap_at_coord` 用 `None`(无 selector)。
    pub target_hint: Option<String>,
}

/// 线程安全的 LRU 账本。多份 `IssuedLedger` 通过 `Arc<Mutex<_>>` 共享同一
/// 底层 `VecDeque` — `App` 内部一份,SDK 端的 `start_capsule_recording` /
/// `stop_capsule_recording_and_reconcile` 复用。
#[derive(Clone, Debug, Default)]
pub struct IssuedLedger {
    inner: Arc<Mutex<VecDeque<IssuedAction>>>,
}

impl IssuedLedger {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(LEDGER_CAP))),
        }
    }

    /// 记一条 act。超 LRU 容量则 pop 最旧的一条。
    pub fn record(&self, action: IssuedAction) {
        let mut q = self.inner.lock().expect("issued ledger mutex poisoned");
        if q.len() >= LEDGER_CAP {
            q.pop_front();
        }
        q.push_back(action);
    }

    pub fn record_tap(&self, ts_ms: f64, target_hint: Option<String>) {
        self.record(IssuedAction {
            kind: IssuedKind::Tap,
            timestamp_ms: ts_ms,
            target_hint,
        });
    }

    pub fn record_fill(&self, ts_ms: f64, target_hint: Option<String>) {
        self.record(IssuedAction {
            kind: IssuedKind::Fill,
            timestamp_ms: ts_ms,
            target_hint,
        });
    }

    pub fn record_tap_at_coord(&self, ts_ms: f64, nx: f64, ny: f64) {
        self.record(IssuedAction {
            kind: IssuedKind::TapAtCoord { nx, ny },
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    pub fn record_swipe_at_coord(&self, ts_ms: f64, from: (f64, f64), to: (f64, f64)) {
        self.record(IssuedAction {
            kind: IssuedKind::SwipeAtCoord { from, to },
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    pub fn record_capsule_start(&self, ts_ms: f64) {
        self.record(IssuedAction {
            kind: IssuedKind::CapsuleStart,
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    /// v5.9 c2 — record a fixture-side action anchor. `action_id` is the
    /// human-readable hint surfaced under [`IssuedAction::target_hint`]
    /// for diagnostics; reconcile only matches by timestamp window.
    pub fn record_fixture_action(&self, ts_ms: f64, action_id: String) {
        self.record(IssuedAction {
            kind: IssuedKind::FixtureAction(action_id.clone()),
            timestamp_ms: ts_ms,
            target_hint: Some(action_id),
        });
    }

    /// 快照拷贝当前账本(用于 `reconcile` 调用 + 单元测试断言)。
    pub fn get_all(&self) -> Vec<IssuedAction> {
        let q = self.inner.lock().expect("issued ledger mutex poisoned");
        q.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut q = self.inner.lock().expect("issued ledger mutex poisoned");
        q.clear();
    }
}
