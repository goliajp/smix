//! Do the examples printed in the guides actually run?
//!
//! Twice on 2026-07-22 the answer turned out to be no, and neither time
//! did a gate catch it — both surfaced only from executing them by
//! hand. The actions guide paired `dispatch: daemonProxy` with an `id`
//! selector the driver refused on every call; the authoring help
//! printed a bare-string example that searched only `text`, while a
//! real iOS tree carries labels and no text at all.
//!
//! Three gates already read these files. One checks the flags exist,
//! one checks the yaml parses, one checks for dead links and noise. The
//! daemonProxy example passed all three and failed on execution, which
//! is the shape of the gap this closes.
//!
//! # Why this is a Rust test and not a python scan
//!
//! Everything that decides whether an example runs is a Rust function:
//! the parser, the runtime's dispatch match, the driver's selector
//! guard, the port chain, the authoring matcher. Calling them directly
//! means the gate judges the real path. A python scan would have to
//! restate those rules, and would then be verifying its own restatement
//! — one contract with two implementations, which is the drift the
//! whole v2.3 segment was spent undoing.
//!
//! # Why it lives in the binary crate
//!
//! Three of the deciding functions are private to `smix-cli`, and
//! `smix-cli` has no lib target. Adding one so a test could reach them
//! would publish a library to crates.io for test convenience.
//!
//! # WHAT THIS CANNOT SEE
//!
//! * Anything that needs a device. It judges whether an example is
//!   admissible up to the wire — whether the runtime and the driver
//!   accept it — not whether the element is on screen.
//! * Whether a summary of a defect is accurate. It checks what a
//!   documented example does; the record of what someone believed about
//!   it is a separate thing, and has been wrong four times this week
//!   while the underlying claims held.
//! * Prose. A guide can describe a behaviour in words that no example
//!   exercises, and this says nothing about it.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use smix_adapter_maestro::AppLike;
use smix_error::ExpectationFailure;
use smix_input::{KeyName, SwipeDirection};
use smix_selector::Selector;

/// A 1x1 truecolour PNG, generated once and pasted here.
///
/// Small enough to read at a glance and real enough to decode, which
/// is the whole requirement: the frame comparison only needs two
/// decodable images that are identical.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0x60, 0x60, 0x00,
    0x00, 0x00, 0x04, 0x00, 0x01, 0xf6, 0x17, 0x38, 0x55, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A device call an example reached, kept so the second arm can re-judge
/// it against the boundary rules.
#[derive(Clone, Debug)]
pub enum MockCall {
    /// `POST /tap` with a mode — the runner-side route.
    TapWithMode(Selector, smix_sdk::TapMode),
    /// `POST /tap-by-id`.
    TapXcui(String),
    /// Host-resolved tap; never reaches a selector-carrying route.
    Tap(Selector),
    DoubleTap(Selector),
    LongPress(Selector),
    /// Everything a launch put on the wire, joined — enough to ask
    /// whether a configured value reached the device at all.
    Launch(String),
}

/// Records what an example asked the device to do, and says yes to
/// everything.
///
/// Saying yes is deliberate: the question is whether the example is
/// *admissible*, not whether a particular screen happens to satisfy it.
/// A mock that failed on absent elements would report every example as
/// broken and prove nothing.
///
/// Close in shape to the mock in smix-adapter-maestro's tests, and not
/// shared with it: a `tests/` directory cannot be imported across
/// crates. This duplicates a mock, not a contract — the contract stays
/// in one place, which is why the second arm calls the real guard
/// rather than re-stating it.
#[derive(Default)]
pub struct MockApp {
    pub calls: Mutex<Vec<MockCall>>,
}

impl MockCall {
    /// How a failure names this call.
    ///
    /// The dispatch mode is the point of half these examples — an
    /// `id` selector is fine on the default path and refused on the
    /// runner-side one — so a message that omitted it would send the
    /// reader back to the guide to work out which they were looking at.
    fn describe(&self) -> String {
        match self {
            MockCall::TapWithMode(s, mode) => format!(
                "tapOn {} (dispatch: {})",
                smix_sdk::describe_selector(s),
                match mode {
                    smix_sdk::TapMode::Resolve => "resolve",
                    smix_sdk::TapMode::ResolveAndTap => "xcui",
                    smix_sdk::TapMode::DaemonProxySynthesize => "daemonProxy",
                }
            ),
            MockCall::TapXcui(id) => format!("tap-by-id {id}"),
            MockCall::Tap(s) => format!("tapOn {} (host-resolved)", smix_sdk::describe_selector(s)),
            MockCall::DoubleTap(s) => format!("doubleTapOn {}", smix_sdk::describe_selector(s)),
            MockCall::LongPress(s) => format!("longPressOn {}", smix_sdk::describe_selector(s)),
            MockCall::Launch(what) => format!("launch {what}"),
        }
    }
}

impl MockApp {
    fn record(&self, call: MockCall) {
        self.calls.lock().expect("mock lock").push(call);
    }
}

#[async_trait::async_trait]
impl AppLike for MockApp {
    async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.record(MockCall::Tap(selector.clone()));
        Ok(())
    }
    async fn tap_xcui(&self, id: &str) -> Result<(), ExpectationFailure> {
        self.record(MockCall::TapXcui(id.to_string()));
        Ok(())
    }
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: smix_sdk::TapMode,
    ) -> Result<(), ExpectationFailure> {
        self.record(MockCall::TapWithMode(selector.clone(), mode));
        Ok(())
    }
    async fn double_tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.record(MockCall::DoubleTap(selector.clone()));
        Ok(())
    }
    async fn long_press(
        &self,
        selector: &Selector,
        _duration: Duration,
    ) -> Result<(), ExpectationFailure> {
        self.record(MockCall::LongPress(selector.clone()));
        Ok(())
    }

    async fn clear_user_defaults(
        &self,
        _bundle_id: &str,
        _keys: &[String],
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn tap_at_coord(&self, _nx: f64, _ny: f64) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    /// Always a hit, centred.
    ///
    /// `None` means "the text is not on screen", which is a fact about
    /// a device this gate does not have. Answering it would report
    /// every documented `ocrText:` example as broken and prove nothing
    /// about whether the example is well-formed.
    async fn find_by_text_ocr(
        &self,
        _text: &str,
        _locales: &[String],
    ) -> Result<Option<smix_sdk::OcrFrame>, ExpectationFailure> {
        Ok(Some(smix_sdk::OcrFrame {
            nx: 0.5,
            ny: 0.5,
            w: 0.1,
            h: 0.05,
        }))
    }
    async fn find_norm_coord(
        &self,
        _selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        Ok(Some((0.5, 0.5)))
    }
    async fn webview_eval(&self, _js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        Ok(serde_json::Value::Null)
    }
    async fn fill(&self, _selector: &Selector, _text: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn press_key(&self, _key: KeyName) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn scroll(
        &self,
        _selector: &Selector,
        _direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn wait_for(
        &self,
        _selector: &Selector,
        _timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn wait_for_not_visible(
        &self,
        _selector: &Selector,
        _timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_visible(&self, _selector: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_not_visible(&self, _selector: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn find(&self, _selector: &Selector) -> Result<bool, ExpectationFailure> {
        Ok(true)
    }
    async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.record(MockCall::Launch(bundle_id.to_string()));
        Ok(())
    }
    async fn terminate(&self, _bundle_id: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn launch_fresh(
        &self,
        bundle_id: &str,
        _clear_state: bool,
        _clear_keychain: bool,
        app_path: Option<&str>,
        launch_arguments: &[String],
    ) -> Result<Vec<String>, ExpectationFailure> {
        self.record(MockCall::Launch(format!(
            "{bundle_id} {} {}",
            app_path.unwrap_or(""),
            launch_arguments.join(" ")
        )));
        Ok(vec![])
    }
    async fn open_url(&self, _url: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn system_popups(&self) -> Result<Vec<smix_sdk::SystemPopup>, ExpectationFailure> {
        Ok(vec![])
    }
    async fn system_popup_action(
        &self,
        _popup_id: &str,
        _button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        Ok(true)
    }
    async fn foreground(&self, _bundle_id: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn launch_app_with_options(
        &self,
        opts: &smix_sdk::LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure> {
        self.record(MockCall::Launch(format!("{opts:?}")));
        Ok(vec![])
    }
    async fn swipe_at_coord(
        &self,
        _from: (f64, f64),
        _to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn scroll_screen(&self, _direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    /// A decodable 1x1 PNG.
    ///
    /// Empty bytes are not "no screenshot", they are a corrupt one:
    /// `waitForAnimationToEnd` decodes what it gets and compares
    /// frames, so an empty vec failed six cookbook examples with a PNG
    /// error that said nothing about the examples.
    async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure> {
        Ok(ONE_PIXEL_PNG.to_vec())
    }
    async fn set_clipboard(&self, _text: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn get_clipboard(&self) -> Result<String, ExpectationFailure> {
        Ok(String::new())
    }
    async fn paste_text(&self, _text: Option<&str>) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn copy_text_from(&self, _selector: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn go_back(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_location(
        &self,
        _latitude: f64,
        _longitude: f64,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn travel(
        &self,
        _points: &[(f64, f64)],
        _speed_mps: Option<f64>,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_permissions(
        &self,
        _bundle_id: &str,
        _permissions: &[(smix_sdk::SimctlPermission, smix_sdk::PermissionAction)],
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn add_media(&self, _paths: &[String]) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_orientation(
        &self,
        _orientation: smix_sdk::MaestroOrientation,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn start_recording(&self, _path: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn stop_recording(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_screenshot(
        &self,
        _baseline_path: &std::path::Path,
        _max_hamming: u32,
    ) -> Result<smix_sdk::AssertScreenshotOutcome, ExpectationFailure> {
        Ok(smix_sdk::AssertScreenshotOutcome::Recorded {
            path: std::path::PathBuf::new(),
        })
    }
}

/// The pages a reader is pointed at, in the order the guide numbers
/// them.
fn guide_pages() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (
            "02-yaml-reference",
            include_str!("../../../docs/ai-guide/02-yaml-reference.md"),
        ),
        (
            "03-selectors",
            include_str!("../../../docs/ai-guide/03-selectors.md"),
        ),
        (
            "04-actions",
            include_str!("../../../docs/ai-guide/04-actions.md"),
        ),
        ("05-cli", include_str!("../../../docs/ai-guide/05-cli.md")),
        (
            "06-fixtures",
            include_str!("../../../docs/ai-guide/06-fixtures.md"),
        ),
        (
            "07-errors",
            include_str!("../../../docs/ai-guide/07-errors.md"),
        ),
        (
            "08-cookbook",
            include_str!("../../../docs/ai-guide/08-cookbook.md"),
        ),
        (
            "10-ai-assertions",
            include_str!("../../../docs/ai-guide/10-ai-assertions.md"),
        ),
    ])
}

fn yaml_blocks(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut cur = String::new();
    for line in doc.lines() {
        if in_block {
            if line.trim_end() == "```" {
                out.push(std::mem::take(&mut cur));
                in_block = false;
            } else {
                cur.push_str(line);
                cur.push('\n');
            }
        } else if line.trim_end() == "```yaml" {
            in_block = true;
        }
    }
    out
}

/// A block carrying its own flow header runs as-is; a bare step list
/// gets the header a reader would put around it.
fn as_flow(block: &str) -> String {
    let is_separator = |l: &str| {
        l.strip_prefix("---")
            .is_some_and(|rest| rest.trim_start().is_empty() || rest.trim_start().starts_with('#'))
    };
    if block.lines().any(is_separator) {
        block.to_string()
    } else {
        format!("appId: com.example.app\n---\n{block}")
    }
}

/// Is this block a flow at all? Config-file shapes (sims.json,
/// `.smix/config.yaml`) live in the same guides and are not the
/// parser's jurisdiction.
fn looks_like_a_flow(block: &str) -> bool {
    // `smix script` files are also yaml lists of steps, and 05-cli
    // shows one. Its entries carry `cmd:`, which no flow verb does.
    if block.lines().any(|l| l.trim_start().starts_with("cmd:")) {
        return false;
    }
    block.contains("- ") || block.contains("appId:")
}

/// The fixture registry 06-fixtures tells the reader to write, read
/// out of the guide itself.
///
/// The `- fixture:` example one block below it names an id, and a
/// registry hand-written here would be a second copy of that block that
/// nothing keeps in step. Loading the printed one means the example and
/// the registry it depends on are checked together — if the guide
/// renames the fixture in one place, this stops resolving.
fn guide_registry() -> smix_fixture::FixtureRegistry {
    let doc = include_str!("../../../docs/ai-guide/06-fixtures.md");
    let mut block = None;
    let mut cur: Option<String> = None;
    for line in doc.lines() {
        match &mut cur {
            Some(buf) if line.trim_end() == "```" => {
                block = Some(std::mem::take(buf));
                cur = None;
            }
            Some(buf) => {
                buf.push_str(line);
                buf.push('\n');
            }
            None if line.trim_end() == "```jsonc" => cur = Some(String::new()),
            None => {}
        }
    }
    let block = block.expect("06-fixtures no longer prints a jsonc registry block");
    // Both arms run the whole corpus, concurrently, in one process. A
    // fixed filename meant one of them reading the file while the other
    // was still writing it.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "smix-guide-gate-fixtures-{}-{n}.json",
        std::process::id()
    ));
    std::fs::write(&path, block).expect("write registry");
    smix_fixture::FixtureRegistry::load(&path).expect("the registry printed in 06-fixtures loads")
}

/// A log tail already carrying the signal the guide's registry waits
/// for.
///
/// `- fixture:` fires a chip and then blocks until a line matching the
/// declared regex appears. On a real run that line comes from the app;
/// here it is put there in advance, because the question is whether the
/// example is well-formed, not whether some app emits the line. Without
/// it the example spends its 8s timeout and reports a failure that says
/// nothing.
fn guide_metro_tail() -> smix_metro_log::MetroLogTail {
    let tail = smix_metro_log::MetroLogTail::new();
    tail.push(
        smix_metro_log::LogLevel::Info,
        "[qa] search-history primed count=5".to_string(),
    );
    tail
}

/// What happened when an example was actually run.
#[derive(Debug)]
enum Verdict {
    /// Reached the device calls listed.
    Reached(Vec<MockCall>),
    /// The runtime refused it before any device call — a parse error, an
    /// unsupported verb, a step that failed on its own terms.
    Refused(String),
}

/// Parse an example and run it against the mock, reporting what it
/// reached.
///
/// The AI tier is force-enabled for its own page: refusing those verbs
/// at parse time unless deliberately turned on is a promise
/// 10-ai-assertions makes, and its examples are written for a reader
/// who has turned it on.
fn run_example(page: &str, block: &str) -> Verdict {
    let flow = {
        smix_adapter_maestro::set_ai_assertions_override(Some(page == "10-ai-assertions"));
        let parsed = smix_adapter_maestro::parse_flow_yaml(&as_flow(block));
        smix_adapter_maestro::set_ai_assertions_override(None);
        match parsed {
            Ok(f) => f,
            Err(e) => return Verdict::Refused(format!("parse: {e}")),
        }
    };
    let app = MockApp::default();
    let base =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/guide-corpus/flows");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let outcome = rt.block_on(async {
        let mut adapter = smix_adapter_maestro::Adapter::new(&app, base)
            .with_fixture_registry(guide_registry())
            .with_metro_tail(guide_metro_tail());
        adapter.run(&flow).await
    });
    remove_artifacts(&flow);
    match outcome {
        Ok(_) => Verdict::Reached(app.calls.lock().expect("mock lock").clone()),
        Err(e) => Verdict::Refused(format!("run: {e}")),
    }
}

/// Delete what an example wrote.
///
/// `takeScreenshot: "step5.png"` resolves against the working
/// directory, which is right for a reader running `smix run` from their
/// project and wrong for a test: running the corpus left a stray
/// `step5.png` in the crate. The paths come from the parsed flow rather
/// than a list here, so an example that starts writing somewhere new is
/// cleaned up without anyone remembering to add it.
fn remove_artifacts(flow: &smix_adapter_maestro::Flow) {
    for step in &flow.steps {
        let path = match step {
            smix_adapter_maestro::Step::TakeScreenshot { path: Some(p), .. } => p,
            smix_adapter_maestro::Step::StartRecording { path } => path,
            _ => continue,
        };
        let _ = std::fs::remove_file(path);
        // write_yaml_output appends an extension when the yaml path has
        // none, so the file on disk may not be the name in the yaml.
        if !std::path::Path::new(path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        {
            let _ = std::fs::remove_file(format!("{path}.png"));
        }
    }
}

/// Examples that do not reach a route, and why.
///
/// A gate that simply went red would have to stay unwired until the
/// last entry was fixed, which is how a red gate becomes a gate nobody
/// runs. Listing them keeps it wired and keeps the list honest in both
/// directions: an unlisted break fails, and so does a listed one that
/// starts working, so closing a finding forces its line out of here.
///
/// Two reasons appear, and they call for opposite responses:
///
/// * `Defect` — the guide prints something the code refuses. Someone
///   has to change one of the two.
/// * `NeedsDevice` — the example is fine and the *gate* cannot judge
///   it, because how it ends depends on what is on screen. Nothing to
///   fix; it is here so the count is not quietly wrong.
#[derive(PartialEq)]
enum Reason {
    Defect,
    NeedsDevice,
}

const KNOWN_BROKEN: &[(&str, usize, Reason, &str)] = &[
    (
        "02-yaml-reference",
        3,
        Reason::Defect,
        "assertTrue: ${output.userCount > 0} — the expression grammar \
         (expr.rs) has no relational operators at all; eq is the \
         tightest comparison it parses. v2.4 finding N6.",
    ),
    (
        "02-yaml-reference",
        7,
        Reason::Defect,
        "pressKey: POWER. v2.4 finding N5.",
    ),
    (
        "02-yaml-reference",
        11,
        Reason::NeedsDevice,
        "repeat.while.visible loops until the element leaves the screen; \
         a mock that always says visible never exits, and one that said \
         otherwise would be inventing a screen.",
    ),
    (
        "04-actions",
        13,
        Reason::Defect,
        "documents pressKey BACK / POWER / SCREEN_LOCK; KeyName has none \
         of the three, and `back` is deliberately not an alias for \
         Delete (runtime.rs parse_key_name). v2.4 finding N5.",
    ),
    (
        "08-cookbook",
        17,
        Reason::Defect,
        "same pressKey BACK, in the Android-only section. v2.4 finding N5.",
    ),
    (
        "10-ai-assertions",
        1,
        Reason::NeedsDevice,
        "assertCondition asks a vision model about the screen. The judge \
         runs; it is looking at a 1x1 pixel.",
    ),
    (
        "10-ai-assertions",
        2,
        Reason::NeedsDevice,
        "extractWithAI reads fields off the screen, then asserts on what \
         it read. Same 1x1 pixel.",
    ),
];

fn known_broken(page: &str, block: usize) -> Option<&'static str> {
    KNOWN_BROKEN
        .iter()
        .find(|(p, b, _, _)| *p == page && *b == block)
        .map(|(_, _, _, why)| *why)
}

/// The list is a claim about how much of the corpus this gate really
/// judges, so it has to be small enough to be worth having.
///
/// Not a round number picked to pass: seven is what it is today, and a
/// ceiling one above it means adding an eighth is a deliberate act
/// rather than a quiet one.
#[test]
fn the_unjudged_list_stays_short() {
    assert!(
        KNOWN_BROKEN.len() <= 8,
        "{} examples the gate does not judge — at some point the list \
         is the finding",
        KNOWN_BROKEN.len()
    );
    let device = KNOWN_BROKEN
        .iter()
        .filter(|(_, _, r, _)| *r == Reason::NeedsDevice)
        .count();
    assert!(
        device <= 3,
        "{device} examples excused as needing a device — each one is a \
         piece of the corpus nothing checks"
    );
}

/// Every yaml example in the guides must reach the device.
///
/// "Reach" is the whole claim: with a mock that says yes to everything,
/// the only way to fail is for the runtime itself to refuse — an
/// unsupported verb, a step shape the parser takes and the runtime does
/// not, a fixture path that does not exist. That is exactly the class
/// the parse gate cannot see, because a block that parses can still
/// have nowhere to go.
#[test]
fn every_yaml_example_reaches_a_route() {
    let mut judged = 0usize;
    let mut broken: Vec<String> = Vec::new();
    let mut fixed: Vec<String> = Vec::new();
    for (page, doc) in guide_pages() {
        for (i, block) in yaml_blocks(doc).iter().enumerate() {
            if !looks_like_a_flow(block) {
                continue;
            }
            judged += 1;
            let listed = known_broken(page, i + 1);
            match (run_example(page, block), listed) {
                (Verdict::Refused(why), None) => {
                    broken.push(format!("{page} block #{}: {why}\n{block}", i + 1))
                }
                (Verdict::Reached(_), Some(why)) => fixed.push(format!(
                    "{page} block #{} now reaches a route — drop its \
                     KNOWN_BROKEN entry:\n  {why}",
                    i + 1
                )),
                _ => {}
            }
        }
    }
    assert!(
        fixed.is_empty(),
        "{} listed-as-broken examples work now:\n\n{}",
        fixed.len(),
        fixed.join("\n\n")
    );
    // 69 today. The floor is close under it rather than comfortably
    // below: a floor with slack in it is a floor that would not have
    // noticed the extraction quietly losing a page.
    assert!(
        judged >= 65,
        "only {judged} flow examples extracted from the guides — the \
         extraction stopped matching and this would pass by knowing \
         nothing"
    );
    assert!(
        broken.is_empty(),
        "{} documented examples do not reach a route:\n\n{}",
        broken.len(),
        broken.join("\n\n")
    );
}

/// Every tap a guide documents must survive the driver's boundary rule.
///
/// The mock is not the driver, so reaching a call proves nothing about
/// whether the real one would take it. This re-judges each recorded
/// selector through the guard the driver actually applies — by calling
/// it, not by restating it. Restating is how the actions guide came to
/// print `dispatch: daemonProxy` with an `id` for as long as it did:
/// the prose said one thing and the function said another, and nothing
/// asked them the same question.
#[test]
fn every_documented_tap_is_admissible_at_the_driver_boundary() {
    let mut checked = 0usize;
    let mut refused: Vec<String> = Vec::new();
    for (page, doc) in guide_pages() {
        for (i, block) in yaml_blocks(doc).iter().enumerate() {
            if !looks_like_a_flow(block) {
                continue;
            }
            let Verdict::Reached(calls) = run_example(page, block) else {
                continue;
            };
            for call in calls {
                let (selector, route) = match &call {
                    MockCall::TapWithMode(s, _) => (s, "/tap"),
                    MockCall::DoubleTap(s) => (s, "/double-tap"),
                    MockCall::LongPress(s) => (s, "/long-press"),
                    // The default tap resolves host-side against the
                    // full tree; no runner-side form applies to it.
                    MockCall::Tap(_) | MockCall::TapXcui(_) | MockCall::Launch(_) => continue,
                };
                checked += 1;
                if let Err(e) = smix_driver::require_runner_resolvable_selector(selector, route) {
                    refused.push(format!(
                        "{page} block #{}: {}\n  → {}",
                        i + 1,
                        call.describe(),
                        e.message
                    ));
                }
            }
        }
    }
    assert!(
        checked > 0,
        "no runner-side tap reached the boundary check — either the \
         guides stopped showing one or the recording stopped working, \
         and this would pass by knowing nothing"
    );
    assert!(
        refused.is_empty(),
        "{} documented taps the driver refuses on every call:\n\n{}",
        refused.len(),
        refused.join("\n\n")
    );
}

/// The negative control: does the judge actually judge?
///
/// A gate written after the thing it guards tends to come out the
/// shape of what already passes. This feeds it an example in exactly
/// the form the guides use — well-formed yaml, real verb, parses
/// cleanly — carrying a selector the driver refuses, and requires both
/// arms to notice. If either stops noticing, the corpus runs are
/// meaningless and this says so.
#[test]
fn the_gate_catches_a_documented_example_the_driver_would_refuse() {
    // A regex pattern: resolvable host-side, and deliberately not on
    // the runner-side routes, which have no pattern semantics.
    //
    // An alternation, because `|` is the only character that actually
    // promotes a string to a pattern. 03-selectors says "auto-detected
    // if string contains regex meta chars" and shows `^Help$` and
    // `Row #[0-9]+`; both of those come out of the parser as literals
    // and match by string equality, which is its own finding and not
    // this test's subject.
    let block = "- tapOn:\n    text: \"Sign In|Log In\"\n    dispatch: daemonProxy\n";
    let Verdict::Reached(calls) = run_example("synthetic", block) else {
        panic!("the injected example did not even reach a route — it was meant to reach one");
    };
    let taps: Vec<_> = calls
        .iter()
        .filter_map(|c| match c {
            MockCall::TapWithMode(s, _) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        taps.len(),
        1,
        "expected one runner-side tap from the injected example, got {calls:?}"
    );
    assert!(
        smix_driver::require_runner_resolvable_selector(taps[0], "/tap").is_err(),
        "the boundary check admitted a regex on a runner-side route — \
         the corpus arm is no longer judging anything"
    );
}

// --------------------------------------------------------------------
// Probes — one per claim in docs/guide-executability.md.
//
// The two arms above judge every example the same way. A probe asks a
// question about one specific claim a guide makes, in the terms that
// claim is written in, and each is named in the list's `probe` column.
// --------------------------------------------------------------------

/// Find the block a probe is about by a string it contains, so that
/// re-ordering a page does not silently point a probe at a different
/// example.
fn block_containing(page: &str, needle: &str) -> String {
    let doc = guide_pages()
        .get(page)
        .copied()
        .unwrap_or_else(|| panic!("{page} is not in the corpus"));
    yaml_blocks(doc)
        .into_iter()
        .find(|b| b.contains(needle))
        .unwrap_or_else(|| panic!("{page} no longer prints a block containing {needle:?}"))
}

/// N1 — every command that dials the runner can reach the registry
/// rung the ladder in `05-cli` documents.
///
/// The page's §Environment-variable precedence says flag, then env,
/// then registry, then default. `smix run` had all four: its
/// `--runner-port` carries `env = "SMIX_RUNNER_PORT"`, so clap merges
/// the env rung before `run_port` sees it, and `--device` is what
/// indexes the registry. The single-shot verbs had flag and env and
/// then stopped — nothing on them named a device, so there was no key
/// to look a `runnerPort` up under, and in a workspace with a sim
/// registered on 22088 `smix tap` dialled 22087 while `smix run`
/// dialled 22088.
///
/// Asked of the clap tree rather than of a list written here, because
/// the first version of this probe asserted `run_port` had no env rung
/// — read off the function signature, without following the `env =`
/// attribute on the argument feeding it. It was wrong, and it was the
/// fifth record of its kind to be wrong that week.
#[test]
fn every_runner_dialling_command_can_reach_the_registry() {
    use clap::CommandFactory;
    let cli = crate::Cli::command();

    // Which commands dial the runner: the ones that take a port.
    // Derived, so a new verb of the same shape is covered on the day it
    // lands rather than when someone remembers to add it.
    let mut blind: Vec<String> = Vec::new();
    let mut checked = 0usize;
    fn walk(cmd: &clap::Command, path: &str, checked: &mut usize, out: &mut Vec<String>) {
        let here = if path.is_empty() {
            cmd.get_name().to_string()
        } else {
            format!("{path} {}", cmd.get_name())
        };
        let args: Vec<&clap::Arg> = cmd.get_arguments().collect();
        if args.iter().any(|a| a.get_id() == "port") {
            *checked += 1;
            let indexes_registry = args
                .iter()
                .any(|a| matches!(a.get_id().as_str(), "device" | "udid" | "alias"));
            if !indexes_registry {
                out.push(here.clone());
            }
        }
        for sub in cmd.get_subcommands() {
            walk(sub, &here, checked, out);
        }
    }
    walk(&cli, "", &mut checked, &mut blind);

    assert!(
        checked >= 8,
        "only {checked} commands take a `--port` — the single-shot verbs \
         were renamed or restructured and this would pass by knowing \
         nothing"
    );
    assert!(
        blind.is_empty(),
        "{} runner-dialling commands cannot name a device, so the \
         registry rung of the documented ladder is unreachable from \
         them:\n  {}",
        blind.len(),
        blind.join("\n  ")
    );

    // The other half of the page's claim, and the shape the fix copies:
    // `smix run` reaches env through clap and the registry through
    // `--device`.
    let run = cli
        .get_subcommands()
        .find(|c| c.get_name() == "run")
        .expect("`smix run` still exists");
    let runner_port = run
        .get_arguments()
        .find(|a| a.get_id() == "runner_port")
        .expect("`smix run --runner-port` still exists");
    assert_eq!(
        runner_port.get_env().and_then(|e| e.to_str()),
        Some("SMIX_RUNNER_PORT"),
        "`smix run` lost its env rung — the ladder is short again, at \
         the other end"
    );
    assert!(
        run.get_arguments().any(|a| a.get_id() == "device"),
        "`smix run --device` is gone — it is what indexes the registry"
    );
}

/// N2 — the Android launch activity a reader configures reaches the
/// device.
///
/// `apps.yaml` takes `activity:`, 08-cookbook shows it being set, and
/// `AndroidApp::activity` even has a default. For a long time
/// resolution then used the package alone and the Kotlin runner started
/// `<pkg>/.MainActivity` regardless — the field was read, defaulted,
/// carried as far as `ResolvedApp::Android`, and dropped. Asked
/// behaviourally: run a flow whose app config names a different
/// activity, and look for that activity in what reached the device.
#[test]
fn a_configured_launch_activity_reaches_the_device() {
    let cfg = smix_adapter_maestro::AppsConfig::from_yaml(
        "apps:\n  demoApp:\n    android:\n      package: com.example.app\n      activity: .NotMainActivity\n",
    )
    .expect("apps config parses");
    let mut flow = smix_adapter_maestro::parse_flow_yaml("app: demoApp\n---\n- launchApp\n")
        .expect("flow parses");
    smix_adapter_maestro::resolve_app_into_flow(&mut flow, &cfg, smix_driver::Platform::Android)
        .expect("resolve");

    let app = MockApp::default();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let mut adapter = smix_adapter_maestro::Adapter::new(
            &app,
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        );
        adapter.run(&flow).await.expect("run");
    });
    let trace: Vec<String> = app
        .calls
        .lock()
        .expect("mock lock")
        .iter()
        .map(MockCall::describe)
        .collect();
    assert!(
        trace.iter().any(|c| c.contains(".NotMainActivity")),
        "the configured activity reaches nothing — it is read from the \
         yaml, defaulted, and dropped before anything is dialled. \
         Trace: {trace:?}"
    );
}

/// N3 — 04-actions says the default tap goes through `/tap-by-id` when
/// the element has an identifier. It does not.
///
/// `IosDriver::tap` fetches the tree, resolves host-side, and calls
/// `/tap-at-norm-coord`; `/tap-by-id` is reached only by
/// `dispatch: xcui` and by the runtime's modal/tab id whitelist. The
/// trace says which, with no reading of prose required.
#[test]
fn the_default_tap_still_misses_the_route_its_page_names() {
    let block = block_containing("04-actions", "home-increment-btn");
    let Verdict::Reached(calls) = run_example("04-actions", &block) else {
        panic!("the default-tap example no longer reaches a route at all");
    };
    let by_id = calls
        .iter()
        .any(|c| matches!(c, MockCall::TapXcui(_) | MockCall::TapWithMode(..)));
    assert!(
        !by_id,
        "the default tap now takes a runner-side route — N3 is fixed \
         and its row must move to `runs`. Trace: {:?}",
        calls.iter().map(MockCall::describe).collect::<Vec<_>>()
    );
}

/// P1 — `dispatch: daemonProxy` with an `id`, the pairing 04-actions
/// has printed since it was written and the driver refused on every
/// call until 2026-07-22.
#[test]
fn the_daemon_proxy_id_example_is_admissible() {
    let block = block_containing("04-actions", "dispatch: daemonProxy");
    let Verdict::Reached(calls) = run_example("04-actions", &block) else {
        panic!("the daemonProxy example no longer reaches a route");
    };
    let taps: Vec<&Selector> = calls
        .iter()
        .filter_map(|c| match c {
            MockCall::TapWithMode(s, smix_sdk::TapMode::DaemonProxySynthesize) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        !taps.is_empty(),
        "the example stopped routing through POST /tap — it is the \
         daemonProxy path or it is not this example"
    );
    for s in taps {
        smix_driver::require_runner_resolvable_selector(s, "/tap").unwrap_or_else(|e| {
            panic!(
                "the driver refuses the documented pairing again: {}",
                e.message
            )
        });
    }
}

/// P2 — the bare-string form `smix authoring suggest` prints, asked
/// against a tree from a real device.
///
/// It used to search `text`, `value` and `title`. The tree below has
/// 33 non-empty labels and no text or title at all, which is what an
/// iOS tree looks like, so the printed example returned nothing on the
/// platform smix exists for. The second half is the injection: the
/// `text:`-qualified form must still find nothing, or the first half
/// would pass on a tree that simply matches everything.
#[test]
fn the_bare_string_form_matches_a_real_tree() {
    let tree: smix_sdk::A11yNode = serde_json::from_str(include_str!(
        "../tests/fixtures/live-tree-preferences-2026-07-22.json"
    ))
    .expect("fixture tree parses");
    let bare = crate::authoring::suggest_selectors(&tree, "General");
    assert!(
        !bare.is_empty(),
        "the bare-string form finds nothing in a real iOS tree again"
    );
    let text_qualified = crate::authoring::suggest_selectors(&tree, "text:General");
    assert!(
        text_qualified.is_empty(),
        "this tree now has a `text` field carrying \"General\" — the \
         fixture changed and this no longer demonstrates anything"
    );
}

// --------------------------------------------------------------------
// The list.
// --------------------------------------------------------------------

const LIST: &str = include_str!("../../../docs/guide-executability.md");
const SELF: &str = include_str!("guide_gate.rs");
const LEDGER: &str = include_str!("../../../docs/audit-ledger.md");

/// One row, already split.
struct Row<'a> {
    id: &'a str,
    status: &'a str,
    probe: &'a str,
    layer: &'a str,
    ledger: &'a str,
    reviewed: &'a str,
}

/// Read the table.
///
/// A row whose cell count is wrong is an error, not a skip. The audit
/// ledger learned this the expensive way: one citation contained an
/// unescaped `|`, the row split into eleven cells, the scan skipped it
/// as unparseable, and reported clean — a row nothing checked, in a
/// table whose entire purpose is that every row is checked.
fn rows() -> Vec<Row<'static>> {
    let mut out = Vec::new();
    for line in LIST.lines() {
        let line = line.trim();
        if !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<&str> = line
            .trim_matches('|')
            .split(" | ")
            // The table is read by people first, so identifiers in it
            // are written as code. The backticks are presentation.
            .map(|c| c.trim().trim_matches('`'))
            .collect();
        let first = cells.first().copied().unwrap_or("");
        // The header and its underline are the only two `|` lines that
        // are not rows.
        if first == "id" || first.starts_with("---") {
            continue;
        }
        assert_eq!(
            cells.len(),
            9,
            "row `{first}` has {} cells, not 9 — escape any `|` inside a \
             cell as `\\|`. A row this reader cannot split is a row \
             nothing checks",
            cells.len()
        );
        out.push(Row {
            id: cells[0],
            status: cells[3],
            probe: cells[4],
            layer: cells[6],
            ledger: cells[7],
            reviewed: cells[8],
        });
    }
    out
}

/// The list and the probes describe the same set of claims.
#[test]
fn the_list_and_the_probes_agree() {
    let rows = rows();
    assert!(
        rows.len() >= 8,
        "only {} rows parsed out of the list — the table shape changed \
         and this check would pass by knowing nothing",
        rows.len()
    );

    let today = "2026-07-22";
    for r in &rows {
        assert!(
            matches!(r.status, "runs" | "broken" | "unjudged"),
            "{}: status `{}` is outside the vocabulary — an open \
             vocabulary drifts this column back into prose",
            r.id,
            r.status
        );
        assert!(
            r.reviewed <= today,
            "{}: reviewed {} is in the future",
            r.id,
            r.reviewed
        );
        if r.status == "runs" {
            assert_eq!(
                r.layer, "—",
                "{}: a claim that runs has no layer to fix",
                r.id
            );
        } else {
            assert_ne!(r.layer, "—", "{}: says what is broken, not where", r.id);
        }
        if r.status == "unjudged" {
            assert_eq!(r.probe, "—", "{}: unjudged rows have no probe", r.id);
        } else {
            assert!(
                SELF.contains(&format!("fn {}(", r.probe)),
                "{}: names probe `{}`, which is not a test in this file",
                r.id,
                r.probe
            );
        }
        if r.ledger != "—" {
            assert!(
                LEDGER.contains(r.ledger),
                "{}: cites ledger row {}, which does not appear in \
                 docs/audit-ledger.md",
                r.id,
                r.ledger
            );
        }
    }

    // Every hand-written probe has a row. Without this, deleting a row
    // would leave its probe running and unaccounted for, which is the
    // half of the drift the row-side check cannot see.
    for name in [
        "every_runner_dialling_command_can_reach_the_registry",
        "a_configured_launch_activity_reaches_the_device",
        "the_default_tap_still_misses_the_route_its_page_names",
        "the_daemon_proxy_id_example_is_admissible",
        "the_bare_string_form_matches_a_real_tree",
        "the_documented_regex_examples_are_still_literals",
    ] {
        assert!(
            SELF.contains(&format!("fn {name}(")),
            "probe `{name}` is named here but no longer exists"
        );
        assert!(
            rows.iter().any(|r| r.probe == name),
            "probe `{name}` runs and no row in the list claims it"
        );
    }
}

/// This gate is only worth having where it actually runs.
///
/// CI and ship both run `cargo test --workspace`, so they pick it up
/// with nothing added. Preflight does not: its crate list comes from
/// `git diff crates/*`, and this gate's input is `docs/`. Editing only
/// a guide — the exact change it guards — skipped it entirely. The
/// three assertions are deliberately different shapes because the three
/// places are different; a uniform "mentions guide_gate" check would
/// have demanded two redundant lines to satisfy itself.
#[test]
fn this_gate_runs_where_it_must() {
    let preflight = include_str!("../../../scripts/dev/preflight.sh");
    assert!(
        preflight.contains("include_str!(\\\"[^\\\"]*$d\\\")"),
        "preflight no longer maps a changed doc back to the crates \
         whose tests read it — edit only a guide and this gate is \
         skipped by the one script meant to predict CI"
    );
    for (name, text) in [
        ("ci.yml", include_str!("../../../.github/workflows/ci.yml")),
        ("ship.sh", include_str!("../../../scripts/release/ship.sh")),
    ] {
        assert!(
            text.contains("cargo test --workspace"),
            "{name} no longer runs the whole workspace, so nothing there \
             runs this gate"
        );
    }
}

/// Print what the corpus and the list came to, for the checkpoint
/// command to read.
///
/// A summary and not an assertion: everything here is checked above.
/// It exists because "the gate is green" says nothing about how much it
/// looked at, and that number is the one worth watching.
#[test]
fn summary() {
    let mut judged = 0usize;
    for (_, doc) in guide_pages() {
        judged += yaml_blocks(doc)
            .iter()
            .filter(|b| looks_like_a_flow(b))
            .count();
    }
    let rows = rows();
    let count = |s: &str| rows.iter().filter(|r| r.status == s).count();
    println!(
        "guide-executability: {} claims ({} runs / {} broken / {} unjudged) \
         · {judged} yaml blocks judged · {} examples not judged ({} of them \
         needing a device)",
        rows.len(),
        count("runs"),
        count("broken"),
        count("unjudged"),
        KNOWN_BROKEN.len(),
        KNOWN_BROKEN
            .iter()
            .filter(|(_, _, r, _)| *r == Reason::NeedsDevice)
            .count(),
    );
}

/// N7 — 03-selectors says a string with regex meta characters becomes a
/// pattern. Only `|` does.
///
/// The page's own examples are `^Help$` and `Row #[0-9]+`; both come out
/// of `text_to_pattern` as literals, and a literal is matched with
/// `eq_ignore_ascii_case`, so each matches exactly the element whose
/// label is that string with the punctuation in it. Nothing errors —
/// the selector simply finds nothing, which is the failure mode the
/// whole segment is about.
///
/// Asked of the two examples the page prints, not of invented ones: if
/// someone changes what the page shows, this should be re-read rather
/// than keep passing on strings nobody documents.
#[test]
fn the_documented_regex_examples_are_still_literals() {
    use smix_selector::Pattern;
    let page = guide_pages()["03-selectors"];
    for shown in ["^Help$", "Row #[0-9]+"] {
        assert!(
            page.contains(shown),
            "03-selectors no longer prints {shown:?} as a regex example \
             — re-read the page before trusting this"
        );
        assert!(
            matches!(
                smix_adapter_maestro::text_to_pattern(shown),
                Pattern::Text(_)
            ),
            "{shown:?} is a pattern now — N7 is fixed and its row must \
             move to `runs`"
        );
    }
    // The contrast: `|` is what actually triggers detection, which is
    // why the negative control at the top of this file uses one.
    assert!(
        matches!(
            smix_adapter_maestro::text_to_pattern("Sign In|Log In"),
            Pattern::Regex { .. }
        ),
        "alternation stopped being detected too — the finding changed \
         shape"
    );
}
