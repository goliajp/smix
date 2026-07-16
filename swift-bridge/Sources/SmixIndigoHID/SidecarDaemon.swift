import Foundation

// Long-running sidecar daemon op set + line parser/formatter.
// Used by the `smix-host-hid daemon` subcommand (stdin JSON-RPC). Each op is
// one newline-delimited JSON object; the parser is hand-rolled so the
// daemon stays free of any serialization framework.

public enum SidecarOp: Equatable {
  case ping
  case tap(udid: String, x: Double, y: Double, path: SmixHostHIDArgs.Path)
  case shutdown
  // Request a host-side AX tree snapshot for the given sim.
  // The daemon constructs the snapshot via AccessibilityPlatformTranslation
  // (dlopen'd at daemon start) and emits a TreeRoute-compatible JSON body.
  case axTree(udid: String)
}

public enum SidecarParseError: Error, Equatable {
  case invalidJson
  case missingField(String)
  case unknownOp(String)
  case invalidPath(String)
}

public enum SidecarDaemon {
  public static func parseLine(_ s: String) throws -> SidecarOp {
    guard let data = s.data(using: .utf8) else { throw SidecarParseError.invalidJson }
    let json: Any
    do {
      json = try JSONSerialization.jsonObject(with: data, options: [])
    } catch {
      throw SidecarParseError.invalidJson
    }
    guard let root = json as? [String: Any] else { throw SidecarParseError.invalidJson }
    guard let op = root["op"] as? String else { throw SidecarParseError.missingField("op") }
    switch op {
    case "ping":
      return .ping
    case "shutdown":
      return .shutdown
    case "tap":
      guard let udid = root["udid"] as? String else { throw SidecarParseError.missingField("udid") }
      guard let x = root["x"] as? Double else { throw SidecarParseError.missingField("x") }
      guard let y = root["y"] as? Double else { throw SidecarParseError.missingField("y") }
      guard let pathRaw = root["path"] as? String else { throw SidecarParseError.missingField("path") }
      guard let path = SmixHostHIDArgs.Path(rawValue: pathRaw) else {
        throw SidecarParseError.invalidPath(pathRaw)
      }
      return .tap(udid: udid, x: x, y: y, path: path)
    case "axTree":
      guard let udid = root["udid"] as? String else { throw SidecarParseError.missingField("udid") }
      return .axTree(udid: udid)
    default:
      throw SidecarParseError.unknownOp(op)
    }
  }

  public static func formatPing(ok: Bool) -> String {
    #"{"ok":\#(ok ? "true" : "false"),"op":"ping"}"#
  }

  public static func formatTap(ok: Bool, path: String, resolved: [String]) -> String {
    let okStr = ok ? "true" : "false"
    let pathEsc = SmixHostJsonEscape.escape(path)
    let resolvedJson = "[" + resolved.map { #""\#(SmixHostJsonEscape.escape($0))""# }.joined(separator: ",") + "]"
    return #"{"ok":\#(okStr),"op":"tap","path":"\#(pathEsc)","resolved":\#(resolvedJson)}"#
  }

  // axTree response. `jsonBody` is pre-serialized A11yNode-shape
  // payload (TreeRoute.serialize output); embedded as a literal sub-object,
  // so callers can `JSON.parse(line).tree` to get the tree directly.
  public static func formatAxTree(ok: Bool, jsonBody: String) -> String {
    let okStr = ok ? "true" : "false"
    return #"{"ok":\#(okStr),"op":"axTree","tree":\#(jsonBody)}"#
  }
}
