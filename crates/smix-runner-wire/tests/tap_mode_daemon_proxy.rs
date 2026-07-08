use smix_runner_wire::TapMode;

#[test]
fn tap_mode_daemon_proxy_serializes_to_camel_case() {
    // Wire-format invariant: this string MUST equal exactly the swift
    // Core raw value (`TapRoute.TapMode.daemonProxySynthesize.rawValue`)
    // so the swift decoder in `TapRoute.decode` parses the host request
    // unchanged. See swift-bridge/Sources/SmixRunnerCore/TapRoute.swift.
    let json = serde_json::to_string(&TapMode::DaemonProxySynthesize).unwrap();
    assert_eq!(json, r#""daemonProxySynthesize""#);
}

#[test]
fn tap_mode_daemon_proxy_roundtrips() {
    let json = serde_json::to_string(&TapMode::DaemonProxySynthesize).unwrap();
    let back: TapMode = serde_json::from_str(&json).unwrap();
    assert_eq!(back, TapMode::DaemonProxySynthesize);
}

#[test]
fn existing_tap_modes_unaffected_by_new_variant() {
    // Backward-compat invariant: `TapMode::Resolve` / `TapMode::ResolveAndTap`
    // wire strings UNCHANGED after the new variant is added.
    assert_eq!(
        serde_json::to_string(&TapMode::Resolve).unwrap(),
        r#""resolve""#
    );
    assert_eq!(
        serde_json::to_string(&TapMode::ResolveAndTap).unwrap(),
        r#""resolveAndTap""#
    );
    let r: TapMode = serde_json::from_str(r#""resolve""#).unwrap();
    assert_eq!(r, TapMode::Resolve);
    let rat: TapMode = serde_json::from_str(r#""resolveAndTap""#).unwrap();
    assert_eq!(rat, TapMode::ResolveAndTap);
}
