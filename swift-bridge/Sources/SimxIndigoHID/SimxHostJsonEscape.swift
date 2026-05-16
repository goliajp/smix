import Foundation

/// Single-point JSON string escape — used by both `SimxHostHIDResult` (tap
/// result wire JSON) and `ChannelProbeReport` (probe wire JSON) so the
/// downstream byte sequence stays byte-identical regardless of producer.
///
/// Behaviour mirrors the original `SimxHostHIDResult.jsonEscape` (C3/C4):
///   - `"` → `\"`
///   - `\` → `\\`
///   - `\n` / `\r` / `\t` → escaped two-byte forms
///   - other control chars (< 0x20) → `\u00XX`
///   - everything else → passthrough
internal enum SimxHostJsonEscape {
  internal static func escape(_ s: String) -> String {
    var out = ""
    out.reserveCapacity(s.count)
    for ch in s {
      switch ch {
      case "\"": out += "\\\""
      case "\\": out += "\\\\"
      case "\n": out += "\\n"
      case "\r": out += "\\r"
      case "\t": out += "\\t"
      default:
        if let scalar = ch.unicodeScalars.first, scalar.value < 0x20 {
          out += String(format: "\\u%04x", scalar.value)
        } else {
          out.append(ch)
        }
      }
    }
    return out
  }
}
