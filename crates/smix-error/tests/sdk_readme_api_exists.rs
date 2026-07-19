//! Every symbol the Swift and Kotlin READMEs name must exist in that
//! SDK's source.
//!
//! Both pages documented a pre-2.0 architecture the SDKs no longer
//! have — `MockSimRuntime`, `MockSelectorResolver`, `SmixSimRuntime`, a
//! `runtime:` argument to `launchApp`, and methods (`launchFresh`,
//! `screenshot`, `openUrl`) that were never written. The quick start,
//! the first code a reader copies, could not compile in either
//! language. Nothing connected the prose to the sources, so the two
//! READMEs agreed with each other and with a version of the SDK that
//! never shipped.
//!
//! This lives in smix-error next to `sdk_failure_code_parity` because
//! it is the same job: the SDKs' user-visible contract, checked against
//! what the code actually declares.

use std::collections::BTreeSet;

const SWIFT_README: &str = include_str!("../../../swift-bridge/README.md");
const KOTLIN_README: &str = include_str!("../../../android-runner/sdk/README.md");

const SWIFT_SOURCES: &[(&str, &str)] = &[
    (
        "App.swift",
        include_str!("../../../swift-bridge/Sources/SmixSDK/App.swift"),
    ),
    (
        "Smix.swift",
        include_str!("../../../swift-bridge/Sources/SmixSDK/Smix.swift"),
    ),
    (
        "Locator.swift",
        include_str!("../../../swift-bridge/Sources/SmixSDK/Locator.swift"),
    ),
    (
        "Selector.swift",
        include_str!("../../../swift-bridge/Sources/SmixSDK/Selector.swift"),
    ),
    (
        "ExpectationFailure.swift",
        include_str!("../../../swift-bridge/Sources/SmixSDK/ExpectationFailure.swift"),
    ),
    // The UniFFI-generated transport the SDK is built on. `SmixDriver`
    // lives here, not in the hand-written facade — a README naming it
    // is naming something real.
    (
        "smix.swift",
        include_str!("../../../swift-bridge/Sources/SmixCoreFFIBindings/Generated/smix.swift"),
    ),
];

const KOTLIN_SOURCES: &[(&str, &str)] = &[
    (
        "App.kt",
        include_str!("../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/App.kt"),
    ),
    (
        "Smix.kt",
        include_str!("../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/Smix.kt"),
    ),
    (
        "Locator.kt",
        include_str!("../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/Locator.kt"),
    ),
    (
        "Selector.kt",
        include_str!("../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/Selector.kt"),
    ),
    (
        "ExpectationFailure.kt",
        include_str!(
            "../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/ExpectationFailure.kt"
        ),
    ),
    (
        "smix.kt",
        include_str!("../../../android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt"),
    ),
];

/// Method names a README calls on `app.` / `loc.` / a bare receiver.
fn methods_called(readme: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in readme.lines() {
        for receiver in ["app.", "loc.", "session.", "welcome."] {
            let mut rest = line;
            while let Some(at) = rest.find(receiver) {
                rest = &rest[at + receiver.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                // Only method calls — a bare property read is not a
                // claim this test can check against a `func` line.
                let is_call = rest[name.len()..].starts_with('(');
                if !name.is_empty() && is_call {
                    out.insert(name);
                }
            }
        }
    }
    out
}

/// Type names a README presents as SDK types: capitalised identifiers
/// used as a constructor or a declared type.
fn types_named(readme: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in readme.lines() {
        // `import SmixSDK` / `package: "smix"` name a MODULE, which no
        // source declares as a type. Checking them turned this into a
        // false alarm about the module the SDK ships as.
        if line.trim_start().starts_with("import") || line.contains("package:") {
            continue;
        }
        for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let starts_upper = token.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            // `Smix*`-prefixed and `Mock*` types are the ones the rotted
            // pages invented; generic words like `Duration` belong to
            // the language, not this SDK.
            if starts_upper && (token.starts_with("Mock") || token.starts_with("Smix")) {
                out.insert(token.to_string());
            }
        }
    }
    out
}

/// Module names are not types. They are derivable rather than listed:
/// Package.swift's products and targets ARE the Swift module names, so
/// `import SmixSDK` and `.product(name: "SmixSDK")` name a module the
/// compiler knows and no source declares.
fn swift_module_names() -> BTreeSet<String> {
    const PACKAGE_SWIFT: &str = include_str!("../../../Package.swift");
    let mut out = BTreeSet::new();
    for marker in [
        ".library(name: \"",
        ".target(name: \"",
        ".testTarget(name: \"",
    ] {
        let mut rest = PACKAGE_SWIFT;
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let name: String = rest.chars().take_while(|c| *c != '"').collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
    out
}

fn declared_anywhere(sources: &[(&str, &str)], symbol: &str) -> bool {
    // Kotlin synthesizes a `<Name>Kt` JVM class for each file's
    // top-level declarations, so `SmixKt` is a real class produced by
    // smix.kt even though no line declares it. The README names it
    // where it explains JNA class loading, correctly.
    if let Some(stem) = symbol.strip_suffix("Kt") {
        let file = format!("{}.kt", stem.to_lowercase());
        if sources
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(&file))
        {
            return true;
        }
    }
    sources.iter().any(|(_, src)| {
        src.contains(&format!("func {symbol}"))
            || src.contains(&format!("fun {symbol}"))
            || src.contains(&format!("class {symbol}"))
            || src.contains(&format!("struct {symbol}"))
            || src.contains(&format!("enum {symbol}"))
            || src.contains(&format!("interface {symbol}"))
            || src.contains(&format!("protocol {symbol}"))
            || src.contains(&format!("object {symbol}"))
            || src.contains(&format!("typealias {symbol}"))
    })
}

fn check(lang: &str, readme: &str, sources: &[(&str, &str)]) -> Vec<String> {
    let modules = swift_module_names();
    let mut missing = Vec::new();
    let methods = methods_called(readme);
    assert!(
        methods.len() >= 5,
        "{lang}: extracted only {} method calls from the README — the \
         extraction stopped matching and this check would pass by \
         knowing nothing",
        methods.len()
    );
    for m in methods {
        if !declared_anywhere(sources, &m) {
            missing.push(format!("{lang}: `{m}(…)` is called but never declared"));
        }
    }
    for t in types_named(readme) {
        if modules.contains(&t) {
            continue;
        }
        if !declared_anywhere(sources, &t) {
            missing.push(format!("{lang}: type `{t}` does not exist"));
        }
    }
    missing
}

#[test]
fn the_swift_readme_only_names_api_the_swift_sdk_has() {
    let missing = check("swift", SWIFT_README, SWIFT_SOURCES);
    assert!(
        missing.is_empty(),
        "the Swift README documents API that is not there:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_kotlin_readme_only_names_api_the_kotlin_sdk_has() {
    let missing = check("kotlin", KOTLIN_README, KOTLIN_SOURCES);
    assert!(
        missing.is_empty(),
        "the Kotlin README documents API that is not there:\n  {}",
        missing.join("\n  ")
    );
}
