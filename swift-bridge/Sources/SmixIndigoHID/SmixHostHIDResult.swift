import Foundation

/// Single-line raw JSON output for the CLI — the host-side smoke script pipes
/// this straight into `jq -e`, so the byte sequence must be stable.
public enum SmixHostHIDResult {
  public static func success(path: String, resolved: [String]) -> String {
    let pathEsc = SmixHostJsonEscape.escape(path)
    let resolvedArr = resolved
      .map { "\"\(SmixHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    return "{\"ok\":true,\"path\":\"\(pathEsc)\",\"resolved\":[\(resolvedArr)]}"
  }

  public static func failure(error: HostHIDError) -> String {
    let code = SmixHostJsonEscape.escape(error.code)
    let detail = SmixHostJsonEscape.escape(error.detail)
    return "{\"ok\":false,\"error\":\"\(code)\",\"detail\":\"\(detail)\"}"
  }
}
