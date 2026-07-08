//! Long visual tour of the fixture app — drives every screen with
//! sustained scrolling, tab switching, modal open/close, and navigation
//! so the /live framebuffer stream has continuous motion to render. Not
//! a capability gate: purely a streaming-quality demo. Loops `--rounds`
//! times (default 3).
//!
//!   cargo run --release --example fixture_tour -- <UDID> [rounds]

use std::process::ExitCode;
use std::time::Duration;

use smix_input::SwipeDirection;
use smix_sdk::{App, id, text};

const BUNDLE: &str = "com.example.app";

async fn pause(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> ExitCode {
    let udid = match std::env::args().nth(1) {
        Some(u) => u,
        None => {
            eprintln!("usage: fixture_tour <UDID> [rounds]");
            return ExitCode::from(64);
        }
    };
    let rounds: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    let port: u16 = std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(22087);
    let app = match App::connect_to_runner(port).await {
        Ok(a) => a.with_udid(&udid),
        Err(e) => {
            eprintln!("connect_to_runner({port}) failed: {}", e.message);
            return ExitCode::from(1);
        }
    };

    // Cold launch into a known state.
    let _ = app.terminate(BUNDLE).await;
    if let Err(e) = app.launch(BUNDLE).await {
        eprintln!("launch failed: {}", e.message);
        return ExitCode::from(1);
    }
    pause(1500).await;

    // Login screen → guest.
    step("login → guest", app.tap(&text("Continue as guest")).await);
    pause(1200).await;

    for r in 1..=rounds {
        eprintln!("=== tour round {r}/{rounds} ===");

        // --- Dashboard: scroll the card list down and back up ---
        step("tab Dashboard", app.tap(&text("Dashboard")).await);
        pause(700).await;
        for _ in 0..3 {
            let _ = app.swipe_once(SwipeDirection::Down).await;
            pause(450).await;
        }
        let _ = app.scroll(&id("card-9"), SwipeDirection::Down).await;
        pause(600).await;
        for _ in 0..3 {
            let _ = app.swipe_once(SwipeDirection::Up).await;
            pause(450).await;
        }

        // --- Search: type, scroll results, open + dismiss modal ---
        step("tab Search", app.tap(&text("Search")).await);
        pause(700).await;
        let _ = app.fill(&id("input-search"), "alpha").await;
        pause(600).await;
        for _ in 0..4 {
            let _ = app.swipe_once(SwipeDirection::Down).await;
            pause(400).await;
        }
        step("open modal", app.tap(&id("btn-open-modal")).await);
        pause(900).await;
        let _ = app.fill(&id("input-modal-name"), "vehicle").await;
        pause(600).await;
        step("modal apply", app.tap(&id("modal-apply")).await);
        pause(800).await;
        for _ in 0..3 {
            let _ = app.swipe_once(SwipeDirection::Up).await;
            pause(400).await;
        }

        // --- Alerts: cycle the three segmented subtabs ---
        step("tab Alerts", app.tap(&text("Alerts")).await);
        pause(700).await;
        for sub in ["Events", "Dwell", "Counting"] {
            let _ = app.tap(&text(sub)).await;
            pause(700).await;
            let _ = app.swipe_once(SwipeDirection::Down).await;
            pause(450).await;
        }

        // --- Settings: toggles, push About detail, pop back ---
        step("tab Settings", app.tap(&text("Settings")).await);
        pause(700).await;
        let _ = app.tap(&id("toggle-dark")).await;
        pause(800).await;
        let _ = app.tap(&id("toggle-dark")).await;
        pause(600).await;
        let _ = app.tap(&id("toggle-notif")).await;
        pause(600).await;
        step("push About", app.tap(&id("link-about")).await);
        pause(1000).await;
        let _ = app.go_back().await;
        pause(800).await;

        // --- Deeplinks: bounce across screens via custom URL scheme ---
        for url in [
            "selftest://search",
            "selftest://alerts",
            "selftest://settings",
            "selftest://home",
        ] {
            let _ = app.open_url(url).await;
            pause(900).await;
        }
    }

    eprintln!("tour complete: {rounds} round(s)");
    let _ = app.terminate(BUNDLE).await;
    ExitCode::SUCCESS
}

fn step(label: &str, r: Result<(), smix_sdk::ExpectationFailure>) {
    match r {
        Ok(()) => eprintln!("  ✓ {label}"),
        Err(e) => eprintln!("  · {label}: {} (continuing)", e.message),
    }
}
