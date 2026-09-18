import Foundation

/// The active Xcode's developer directory, as `xcode-select -p` reports
/// it. CoreSimulator's `sharedServiceContextForDeveloperDir:` wants
/// exactly this value, and SimulatorKit's location is derived from it —
/// so every consumer asks here rather than carrying its own copy of
/// `/Applications/Xcode.app/Contents/Developer`, which is only true on
/// machines where Xcode was installed there.
public enum DeveloperDir {
  public enum Error: Swift.Error, Equatable, CustomStringConvertible {
    case spawn(String)
    case exit(Int32)
    case notUTF8
    case empty

    public var description: String {
      switch self {
      case .spawn(let detail): return "spawn xcode-select: \(detail)"
      case .exit(let status): return "xcode-select -p exit \(status)"
      case .notUTF8: return "xcode-select -p output is not UTF-8"
      case .empty: return "xcode-select -p returned an empty path"
      }
    }
  }

  /// Pure: what `xcode-select -p` printed and how it exited, judged.
  public static func parse(stdout: Data, status: Int32) throws -> String {
    if status != 0 { throw Error.exit(status) }
    guard let s = String(data: stdout, encoding: .utf8) else { throw Error.notUTF8 }
    let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.isEmpty { throw Error.empty }
    return trimmed
  }

  /// `/usr/bin/xcode-select -p`, judged by `parse`.
  public static func current() throws -> String {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/xcode-select")
    proc.arguments = ["-p"]
    let pipe = Pipe()
    proc.standardOutput = pipe
    proc.standardError = Pipe()
    do {
      try proc.run()
    } catch {
      throw Error.spawn("\(error)")
    }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    proc.waitUntilExit()
    return try parse(stdout: data, status: proc.terminationStatus)
  }
}
