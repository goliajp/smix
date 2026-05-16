import Foundation

/// Single-line raw JSON output for the CLI — the host-side smoke script pipes
/// this straight into `jq -e`, so the byte sequence must be stable.
public enum SimxHostHIDResult {
  public static func success(path: String, resolved: [String]) -> String {
    let pathEsc = SimxHostJsonEscape.escape(path)
    let resolvedArr = resolved
      .map { "\"\(SimxHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    return "{\"ok\":true,\"path\":\"\(pathEsc)\",\"resolved\":[\(resolvedArr)]}"
  }

  public static func failure(error: HostHIDError) -> String {
    let code = SimxHostJsonEscape.escape(error.code)
    let detail = SimxHostJsonEscape.escape(error.detail)
    return "{\"ok\":false,\"error\":\"\(code)\",\"detail\":\"\(detail)\"}"
  }
}
