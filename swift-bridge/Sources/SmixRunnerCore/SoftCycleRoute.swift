import FlyingFox
import Foundation

/// `POST /soft-cycle` response builder. The host-side `try_soft_cycle`
/// depends only on the 200 (it re-confirms recovery with a follow-up
/// `GET /health` after the server bounce); `rebound` / `mode` / `wallMs`
/// are for human / AI log disambiguation.
public enum SoftCycleRoute {
  public static func body(rebound: Bool, mode: String, wallMs: UInt32) -> Data {
    let escapedMode = mode
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    let json = #"{"ok":true,"rebound":\#(rebound),"mode":"\#(escapedMode)","wallMs":\#(wallMs)}"#
    return Data(json.utf8)
  }

  public static func response(rebound: Bool, mode: String, wallMs: UInt32) -> HTTPResponse {
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body(rebound: rebound, mode: mode, wallMs: wallMs)
    )
  }
}
