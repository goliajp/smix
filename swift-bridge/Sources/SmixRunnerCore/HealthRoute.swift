import FlyingFox
import Foundation

public enum HealthRoute {
  // Stable byte sequence — host-side curl jq-parses it.
  public static func body() -> Data {
    return Data(#"{"ok":true}"#.utf8)
  }

  public static func response() -> HTTPResponse {
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body()
    )
  }
}
