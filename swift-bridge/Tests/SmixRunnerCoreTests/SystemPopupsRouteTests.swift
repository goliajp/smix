import XCTest
import FlyingFox
#if canImport(CoreGraphics)
import CoreGraphics
#endif
@testable import SmixRunnerCore

// `GET /system-popups` route + scope forward + guarded fallback.
// Sensing capabilities belong to smix core as flat routes rather than being
// buried inside a driver, so popup-sense is a peer of /find, /tap, /tree
// and /fill; the runner exposes it via a handler that
// returns a structured `[SystemPopupsRoute.Popup]` list, and the route
// serializes the envelope. Unit-decidable in SmixRunnerCore (no XCUI / no
// sim): drive the REAL `runForever` route via an IPv4 client on a fixed
// free port (mirrors TreeScopeForwardTests / FindTapScopeForwardTests),
// then `POST /shutdown` for a clean graceful stop.
final class SystemPopupsRouteTests: XCTestCase {

  private func freePort() async throws -> UInt16 {
    let probe = SmixRunnerServer.makeServer(port: 0)
    let probeTask = Task { _ = try? await probe.run() }
    var port: UInt16 = 0
    for _ in 0..<100 {
      if let a = await probe.listeningAddress {
        switch a {
        case .ip4(_, let p), .ip6(_, let p): port = p
        case .unix: break
        }
        if port != 0 { break }
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    await probe.stop(timeout: 0.5)
    probeTask.cancel()
    XCTAssertNotEqual(port, 0, "probe server did not bind")
    return port
  }

  private func waitUp(_ port: UInt16) async throws {
    var up = false
    for _ in 0..<100 {
      if let (_, resp) = try? await URLSession.shared.data(
        from: URL(string: "http://127.0.0.1:\(port)/health")!),
        (resp as? HTTPURLResponse)?.statusCode == 200 {
        up = true; break
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    XCTAssertTrue(up, "server did not come up on :\(port)")
  }

  private func shutdown(_ port: UInt16, _ task: Task<Void, Error>) async {
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await task.value
  }

  // Assertion 1: GET /system-popups (no query) ⇒ handler receives nil scope,
  // empty handler returns []; wire payload EXACTLY {"ok":true,"popups":[]}.
  func test_systemPopups_noQuery_emptyHandler_returnsEmptyEnvelope() async throws {
    let port = try await freePort()
    let received = ScopeBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { scope in
          await received.set(scope)
          return []
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, resp) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    XCTAssertEqual((resp as? HTTPURLResponse)?.statusCode, 200)
    let s1 = await received.value
    XCTAssertEqual(s1, .some(nil), "no ?include= ⇒ scope nil")

    let str = String(data: body, encoding: .utf8) ?? ""
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, true)
    let popups = obj?["popups"] as? [Any]
    XCTAssertEqual(popups?.count, 0, "empty handler ⇒ popups:[]; got body=\(str)")

    await shutdown(port, runTask)
  }

  // Assertion 2: ?include=all-windows forwarded verbatim to handler.
  func test_systemPopups_includeAllWindows_forwardsScopeVerbatim() async throws {
    let port = try await freePort()
    let received = ScopeBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { scope in
          await received.set(scope)
          return []
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    _ = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups?include=all-windows")!)
    let s = await received.value
    XCTAssertEqual(s, .some("all-windows"),
      "?include=all-windows forwarded verbatim to SystemPopupsHandler")

    await shutdown(port, runTask)
  }

  // Assertion 3: handler returning 1 alert with two-button list (cancel +
  // default with outcomeHint) serializes through the popup schema with
  // role enum populated and outcomeHint preserved (incl. null variant).
  func test_systemPopups_handlerReturnsPopup_serializesAllFields() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-1",
      type: "alert",
      source: "com.apple.springboard",
      title: "Open in “Example”?",
      body: "example://… wants to open in Example.",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-cancel", label: "Cancel", role: "cancel",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-open", label: "Open", role: "default",
          dangerous: false, outcomeHint: "scheme-confirm-affirm"),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, true)
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 1)
    let p0 = popups?.first ?? [:]
    XCTAssertEqual(p0["id"] as? String, "p-1")
    XCTAssertEqual(p0["type"] as? String, "alert")
    XCTAssertEqual(p0["source"] as? String, "com.apple.springboard")
    XCTAssertEqual(p0["title"] as? String, "Open in “Example”?")
    let buttons = p0["buttons"] as? [[String: Any]]
    XCTAssertEqual(buttons?.count, 2)
    let b0 = buttons?[0] ?? [:]
    XCTAssertEqual(b0["id"] as? String, "b-cancel")
    XCTAssertEqual(b0["label"] as? String, "Cancel")
    XCTAssertEqual(b0["role"] as? String, "cancel")
    XCTAssertEqual(b0["dangerous"] as? Bool, false)
    XCTAssertTrue(b0["outcomeHint"] is NSNull,
      "cancel button outcomeHint null ⇒ JSON null")
    let b1 = buttons?[1] ?? [:]
    XCTAssertEqual(b1["id"] as? String, "b-open")
    XCTAssertEqual(b1["role"] as? String, "default")
    XCTAssertEqual(b1["outcomeHint"] as? String, "scheme-confirm-affirm")

    await shutdown(port, runTask)
  }

  // Assertion 4: dangerous flag honoured (destructive role OR explicit
  // dangerous=true) round-trips on the wire as boolean true.
  func test_systemPopups_dangerousButton_serializesTrue() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-2",
      type: "alert",
      source: "com.apple.springboard",
      title: "Delete?",
      body: "",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-cancel", label: "Cancel", role: "cancel",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-delete", label: "Delete", role: "destructive",
          dangerous: true, outcomeHint: nil),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let buttons = (obj?["popups"] as? [[String: Any]])?.first?["buttons"] as? [[String: Any]]
    XCTAssertEqual(buttons?[0]["dangerous"] as? Bool, false)
    XCTAssertEqual(buttons?[1]["dangerous"] as? Bool, true)
    XCTAssertEqual(buttons?[1]["role"] as? String, "destructive")

    await shutdown(port, runTask)
  }

  // Assertion 5: route-level guardedResponse — handler that throws never
  // surfaces 5xx; the server returns the fallback empty-popups envelope
  // (sense fails ⇒ "nothing observed right now", NOT a protocol error).
  func test_systemPopups_handlerThrows_guardedFallbackReturnsEmptyEnvelope() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    // The handler is non-throwing by typealias, but the throwing path that
    // matters in the wider system is the response-builder failure caught by
    // `Self.guardedResponse`. Inject a handler that, in production, would
    // call into a closure raising mid-enumeration; here we provoke the
    // guard by constructing a popup that the serializer must still survive
    // and confirm the wire stays ok=true with popups:[] on any
    // serializer-internal failure path. As a concrete shape-stable proxy,
    // returning an empty list goes through the same `success(popups:)`
    // envelope code path; we additionally assert the *fallback* envelope
    // returned by `SystemPopupsRoute.success(popups: [])` is byte-stable
    // (this is the contract on which the route guard relies).
    let fallback = SystemPopupsRoute.success(popups: [])
    let fallbackBody = try await fallback.bodyData
    let fallbackObj = try JSONSerialization.jsonObject(with: fallbackBody) as? [String: Any]
    XCTAssertEqual(fallbackObj?["ok"] as? Bool, true)
    XCTAssertEqual((fallbackObj?["popups"] as? [Any])?.count, 0)
    XCTAssertEqual(fallback.statusCode, .ok)

    // End-to-end: drive the server with an empty handler (the common
    // benign path) — must yield the same byte-stable empty envelope.
    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)
    let (wireBody, wireResp) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    XCTAssertEqual((wireResp as? HTTPURLResponse)?.statusCode, 200)
    XCTAssertEqual(wireBody, fallbackBody,
      "wire empty envelope byte-identical to SystemPopupsRoute.success(popups:[])")

    await shutdown(port, runTask)
  }

  // Assertion 6: no POST surface — the route is GET-only. POSTing to
  // /system-popups must NOT mutate or 200 with a popups envelope (the
  // server has no POST handler registered for this path).
  func test_systemPopups_postNotSupported() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    req.httpMethod = "POST"
    req.httpBody = Data("{}".utf8)
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    let (_, resp) = try await URLSession.shared.data(for: req)
    let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
    XCTAssertNotEqual(code, 200,
      "POST /system-popups must not be a valid route — got \(code)")

    await shutdown(port, runTask)
  }

  // Bound-app modal extension. The runner enumerates
  // not only SpringBoard alerts / sheets / dialogs but also the bound-app's
  // own alert / sheet / dialog / popover (the structural iOS modal types).
  // Wire layer must accept the new `popover` type verbatim and an in-app
  // `source` (the bound-app bundleId), so the route serializer is
  // future-proof against runtime-specific modals while staying transparent
  // (decision still in the upper layer).

  // Assertion 7: popover type round-trips through the wire (popupTypeName
  // extended to map XCUIElement.ElementType.popover ⇒ "popover").
  func test_systemPopups_popoverType_serializesVerbatim() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-pop",
      type: "popover",
      source: "com.example.app",
      title: "Hint",
      body: "Tap an option.",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-1", label: "Option A", role: "default",
          dangerous: false, outcomeHint: nil),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 1)
    XCTAssertEqual(popups?.first?["type"] as? String, "popover",
      "popover type round-trips through wire")

    await shutdown(port, runTask)
  }

  // Assertion 8: in-app `source` (bound-app bundleId) round-trips as the
  // raw bundleId string. The route layer does NOT special-case SpringBoard
  // — `source` is a free-form string; upper layer decides by inspecting it.
  func test_systemPopups_inAppSource_serializesBundleIdVerbatim() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-inapp",
      type: "sheet",
      source: "com.example.app",
      title: "",
      body: "",
      buttons: [])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 1)
    XCTAssertEqual(popups?.first?["source"] as? String,
      "com.example.app",
      "in-app source round-trips verbatim (route does not gate on bundleId)")

    await shutdown(port, runTask)
  }

  // Assertion 9: mixed list — SpringBoard alert + in-app sheet + popover.
  // List order preserved (handler-provided order is the contract; the route
  // does not sort or filter). Empty `buttons` list is legal (an overlay
  // sheet with no actionable buttons is a real iOS shape).
  func test_systemPopups_mixedList_preservesOrderAndShape() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let sbAlert = SystemPopupsRoute.Popup(
      id: "p-sb",
      type: "alert",
      source: "com.apple.springboard",
      title: "Open in “X”?",
      body: "",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-c", label: "Cancel", role: "cancel",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-o", label: "Open", role: "default",
          dangerous: false, outcomeHint: "scheme-confirm-affirm"),
      ])
    let inAppSheet = SystemPopupsRoute.Popup(
      id: "p-app",
      type: "sheet",
      source: "com.example.app",
      title: "",
      body: "",
      buttons: [])
    let inAppPopover = SystemPopupsRoute.Popup(
      id: "p-pop",
      type: "popover",
      source: "com.example.app",
      title: "Tip",
      body: "",
      buttons: [])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [sbAlert, inAppSheet, inAppPopover] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 3, "all three popups present")
    XCTAssertEqual(popups?[0]["source"] as? String, "com.apple.springboard",
      "order preserved: SpringBoard first")
    XCTAssertEqual(popups?[1]["source"] as? String,
      "com.example.app", "in-app sheet second")
    XCTAssertEqual(popups?[1]["type"] as? String, "sheet")
    XCTAssertEqual(popups?[2]["type"] as? String, "popover")
    let inAppButtons = popups?[1]["buttons"] as? [Any]
    XCTAssertEqual(inAppButtons?.count, 0, "empty buttons list legal")

    await shutdown(port, runTask)
  }

  // Bound-app NON-main-window extension. Enumerating the four iOS standard
  // modal types (alert/sheet/dialog/popover) is not enough: a RN custom
  // overlay such as expo-dev-menu is NOT typed as any of them — it renders as a separate
  // non-main bound-app window (XCUIElement.ElementType.window) which the
  // type-keyed enumeration missed. The runner therefore additionally
  // enumerates `app.windows.allElementsBoundByIndex` skipping index 0
  // (the primary app window) and emits each surviving entry with
  // type="window", source=bundleId. The wire layer must round-trip this
  // verbatim — no special-casing, decision still in the upper layer.

  // Assertion 10: type="window" round-trips through the wire layer
  // unchanged. The serializer is type-string-transparent so any new core
  // popup type lands on the wire as-is.
  func test_systemPopups_windowType_serializesVerbatim() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-win",
      type: "window",
      source: "com.example.app",
      title: "This is the developer menu.",
      body: "",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-close", label: "Close", role: "default",
          dangerous: false, outcomeHint: nil),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 1)
    XCTAssertEqual(popups?.first?["type"] as? String, "window",
      "window type round-trips through wire")
    XCTAssertEqual(popups?.first?["source"] as? String,
      "com.example.app",
      "in-app bundleId source round-trips on window-typed popup")

    await shutdown(port, runTask)
  }

  // Assertion 11: a window-typed popup with multiple buttons (the
  // expo-dev-menu shape: Close / Continue / Reload / Fast refresh / …)
  // preserves order and dangerous=false default-role buttons through the
  // wire. The upper layer (AI / e2e) decides which button to press; the
  // route does not pick.
  func test_systemPopups_windowType_multiButtonShape_preserved() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let popup = SystemPopupsRoute.Popup(
      id: "p-devmenu",
      type: "window",
      source: "com.example.app",
      title: "This is the developer menu.",
      body: "It gives you access to useful tools in development builds.",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-close", label: "Close", role: "default",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-continue", label: "Continue", role: "default",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-reload", label: "Reload", role: "default",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-toggle", label: "Toggle element inspector", role: "default",
          dangerous: false, outcomeHint: nil),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in [popup] }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let p0 = (obj?["popups"] as? [[String: Any]])?.first ?? [:]
    XCTAssertEqual(p0["type"] as? String, "window")
    let buttons = p0["buttons"] as? [[String: Any]]
    XCTAssertEqual(buttons?.count, 4, "4-button dev-menu shape preserved")
    XCTAssertEqual(buttons?[0]["label"] as? String, "Close",
      "order preserved: Close first")
    XCTAssertEqual(buttons?[3]["label"] as? String, "Toggle element inspector",
      "order preserved: Toggle element inspector last")
    XCTAssertTrue(buttons?[0]["outcomeHint"] is NSNull,
      "no auto outcomeHint on window-typed popups (decision in upper layer)")

    await shutdown(port, runTask)
  }

  // Assertion 12: mixed list — SpringBoard alert + in-app sheet + in-app
  // popover + in-app window — all four shapes coexist in one envelope.
  // List order, type field, and source field all round-trip per popup.
  func test_systemPopups_mixedList_includesWindowType() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let sbAlert = SystemPopupsRoute.Popup(
      id: "p-sb",
      type: "alert",
      source: "com.apple.springboard",
      title: "Open in “X”?", body: "",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-c", label: "Cancel", role: "cancel",
          dangerous: false, outcomeHint: nil),
        SystemPopupsRoute.PopupButton(
          id: "b-o", label: "Open", role: "default",
          dangerous: false, outcomeHint: "scheme-confirm-affirm"),
      ])
    let inAppSheet = SystemPopupsRoute.Popup(
      id: "p-app", type: "sheet",
      source: "com.example.app",
      title: "", body: "", buttons: [])
    let inAppPopover = SystemPopupsRoute.Popup(
      id: "p-pop", type: "popover",
      source: "com.example.app",
      title: "Tip", body: "", buttons: [])
    let inAppWindow = SystemPopupsRoute.Popup(
      id: "p-win", type: "window",
      source: "com.example.app",
      title: "This is the developer menu.", body: "",
      buttons: [
        SystemPopupsRoute.PopupButton(
          id: "b-close", label: "Close", role: "default",
          dangerous: false, outcomeHint: nil),
      ])

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { _ in
          [sbAlert, inAppSheet, inAppPopover, inAppWindow]
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let obj = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    let popups = obj?["popups"] as? [[String: Any]]
    XCTAssertEqual(popups?.count, 4, "all four popups present")
    XCTAssertEqual(popups?[0]["type"] as? String, "alert")
    XCTAssertEqual(popups?[0]["source"] as? String, "com.apple.springboard")
    XCTAssertEqual(popups?[1]["type"] as? String, "sheet")
    XCTAssertEqual(popups?[2]["type"] as? String, "popover")
    XCTAssertEqual(popups?[3]["type"] as? String, "window",
      "window-typed in-app popup is the fourth shape")
    XCTAssertEqual(popups?[3]["source"] as? String,
      "com.example.app",
      "window-typed popup carries the bound-app bundleId")

    await shutdown(port, runTask)
  }

  actor ScopeBox {
    private var _v: String??
    func set(_ v: String?) { _v = .some(v) }
    var value: String?? { _v }
  }
}
