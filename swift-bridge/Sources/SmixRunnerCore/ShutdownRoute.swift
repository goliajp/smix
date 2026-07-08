import FlyingFox
import Foundation

public enum ShutdownRoute {
  // Stable byte sequence — host-side gracefulStopRunner depends only on the
  // 200; the `op` field is for human / AI log disambiguation.
  public static func body() -> Data {
    return Data(#"{"ok":true,"op":"shutdown"}"#.utf8)
  }

  public static func response() -> HTTPResponse {
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body()
    )
  }
}
