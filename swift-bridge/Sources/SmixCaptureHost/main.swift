// smix-capture-host — simulator framebuffer reader.
//
// Two modes, selected by argv[2]:
//
//   stream mode:  <UDID> <FPS>
//     stderr: "<W>x<H>\n" geometry header (one line), then any error text
//     stdout: contiguous BGRA frames, exactly W*H*4 bytes each, row padding
//             stripped, at the target FPS. Used by the /live video pipeline.
//
//   serve mode:   <UDID> serve
//     A resident request-response capturer for the screenshot path. Resolves
//     the display IOSurface once, then answers one frame per request byte on
//     stdin:
//       request  : one opcode byte — 'R' (raw BGRA) or 'P' (ImageIO PNG).
//                  stdin EOF ends the process.
//       response : one status byte — 0 = ok, 1 = surface unavailable.
//                  on ok: w:u32 h:u32 len:u32 (little endian) + len payload.
//                  on unavailable: the host exits(2); the caller falls back to
//                  `simctl io screenshot`.
//     Correctness: the surface is re-fetched from the device on EVERY grab, so
//     a reboot that vends a new framebuffer is picked up and a stale/garbage
//     frame is never returned.
//
// Apple framework only (Foundation + IOSurface + CoreGraphics + ImageIO). All
// CoreSimulator entry points are resolved via dlopen + objc dynamic lookup, so
// there is no link-time hard dependency on private frameworks.

import CoreGraphics
import Foundation
import IOSurface
import ImageIO

// MARK: - Argv

let argv = CommandLine.arguments
guard argv.count >= 3 else {
  fputs("usage: smix-capture-host <UDID> <FPS|serve>\n", stderr)
  exit(64)
}
let udidArg = argv[1].uppercased()
let modeArg = argv[2]

// MARK: - dlopen CoreSimulator

guard
  dlopen(
    "/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator",
    RTLD_NOW
  ) != nil
else {
  fputs("dlopen CoreSimulator failed\n", stderr)
  exit(1)
}

func call(_ obj: NSObject, _ name: String) -> NSObject? {
  let sel = NSSelectorFromString(name)
  guard obj.responds(to: sel) else { return nil }
  return obj.perform(sel)?.takeUnretainedValue() as? NSObject
}

/// Resolve the CoreSimulator `SimDevice` for `udid`. Held for the process
/// lifetime; the framebuffer surface is fetched from it on demand.
func resolveDevice(udid: String) -> NSObject? {
  guard let ctxClass = NSClassFromString("SimServiceContext") as? NSObject.Type else {
    fputs("SimServiceContext class missing\n", stderr)
    return nil
  }
  let sharedSel = NSSelectorFromString("sharedServiceContextForDeveloperDir:error:")
  typealias SharedCtxFn = @convention(c) (
    AnyObject, Selector, NSString, UnsafeMutablePointer<NSError?>?
  ) -> NSObject?
  let sharedImp = unsafeBitCast(ctxClass.method(for: sharedSel), to: SharedCtxFn.self)
  var err: NSError?
  guard
    let ctx = sharedImp(
      ctxClass, sharedSel,
      "/Applications/Xcode.app/Contents/Developer" as NSString, &err
    )
  else {
    fputs("ctx error: \(String(describing: err))\n", stderr)
    return nil
  }
  let setSel = NSSelectorFromString("defaultDeviceSetWithError:")
  typealias DeviceSetFn = @convention(c) (
    AnyObject, Selector, UnsafeMutablePointer<NSError?>?
  ) -> NSObject?
  let setImp = unsafeBitCast(ctx.method(for: setSel), to: DeviceSetFn.self)
  guard
    let devSet = setImp(ctx, setSel, &err),
    let devices = devSet.value(forKey: "devices") as? [NSObject],
    let dev = devices.first(where: {
      (($0.value(forKey: "UDID") as? UUID)?.uuidString ?? "") == udid
    })
  else {
    fputs("device resolution failed for udid=\(udid)\n", stderr)
    return nil
  }
  return dev
}

/// Fetch the device's CURRENT display framebuffer surface. Called on every
/// grab in serve mode so a rebooted sim's new surface is adopted and a stale
/// one is never read. Returns nil when the sim is not presenting a framebuffer
/// (shut down / mid-reboot) — the caller then reports surface-unavailable.
func currentSurface(_ dev: NSObject) -> IOSurface? {
  guard
    let io = dev.value(forKey: "io") as? NSObject,
    let ports = io.value(forKey: "ioPorts") as? [NSObject]
  else {
    return nil
  }
  for port in ports {
    guard let d = call(port, "descriptor") else { continue }
    if let s = call(d, "framebufferSurface") {
      return unsafeDowncast(s, to: IOSurface.self)
    }
  }
  return nil
}

func resolveSurface(udid: String) -> IOSurface? {
  guard let dev = resolveDevice(udid: udid) else { return nil }
  guard let s = currentSurface(dev) else {
    fputs("no port with a non-nil framebufferSurface\n", stderr)
    return nil
  }
  return s
}

// MARK: - little-endian helpers

@inline(__always)
func leU32(_ v: UInt32) -> [UInt8] {
  [UInt8(v & 0xff), UInt8((v >> 8) & 0xff), UInt8((v >> 16) & 0xff), UInt8((v >> 24) & 0xff)]
}

/// Copy a locked BGRA IOSurface into a contiguous row-stripped buffer.
func copyStripped(_ surface: IOSurface, width: Int, height: Int) -> Data {
  let rowBytes = width * 4
  var out = Data(count: rowBytes * height)
  IOSurfaceLock(surface, [.readOnly], nil)
  let base = IOSurfaceGetBaseAddress(surface)
  let stride = IOSurfaceGetBytesPerRow(surface)
  out.withUnsafeMutableBytes { dst in
    let dstBase = dst.baseAddress!
    for row in 0..<height {
      let src = base.advanced(by: row * stride)
      dstBase.advanced(by: row * rowBytes).copyMemory(from: src, byteCount: rowBytes)
    }
  }
  IOSurfaceUnlock(surface, [.readOnly], nil)
  return out
}

/// ImageIO-encode a locked BGRA IOSurface to a PNG (sRGB-tagged). Returns nil
/// on any CoreGraphics/ImageIO failure.
func encodePNG(_ surface: IOSurface, width: Int, height: Int) -> Data? {
  IOSurfaceLock(surface, [.readOnly], nil)
  defer { IOSurfaceUnlock(surface, [.readOnly], nil) }
  let base = IOSurfaceGetBaseAddress(surface)
  let stride = IOSurfaceGetBytesPerRow(surface)
  guard let cs = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
  // BGRA8888 in memory = 32-bit little-endian, alpha first.
  let bitmapInfo = CGImageAlphaInfo.premultipliedFirst.rawValue
    | CGBitmapInfo.byteOrder32Little.rawValue
  guard
    let ctx = CGContext(
      data: base, width: width, height: height, bitsPerComponent: 8,
      bytesPerRow: stride, space: cs, bitmapInfo: bitmapInfo),
    let cg = ctx.makeImage()
  else { return nil }
  let data = NSMutableData()
  guard
    let dest = CGImageDestinationCreateWithData(
      data as CFMutableData, "public.png" as CFString, 1, nil)
  else { return nil }
  CGImageDestinationAddImage(dest, cg, nil)
  guard CGImageDestinationFinalize(dest) else { return nil }
  return data as Data
}

// MARK: - Serve mode

if modeArg == "serve" {
  guard let dev = resolveDevice(udid: udidArg) else { exit(1) }
  guard var surface = currentSurface(dev) else {
    fputs("no port with a non-nil framebufferSurface\n", stderr)
    exit(1)
  }
  fputs("\(IOSurfaceGetWidth(surface))x\(IOSurfaceGetHeight(surface))\n", stderr)
  fflush(stderr)

  let stdinFh = FileHandle.standardInput
  let stdoutFh = FileHandle.standardOutput

  func emit(_ bytes: [UInt8]) -> Bool {
    do {
      try stdoutFh.write(contentsOf: Data(bytes))
      return true
    } catch { return false }
  }
  func emit(_ data: Data) -> Bool {
    do {
      try stdoutFh.write(contentsOf: data)
      return true
    } catch { return false }
  }

  while true {
    let req = stdinFh.readData(ofLength: 1)
    if req.isEmpty { exit(0) }  // stdin closed → caller dropped us
    let op = req[req.startIndex]

    // Revalidate the surface on every grab: pick up a rebooted sim's new
    // framebuffer, or report unavailable if the sim is no longer presenting.
    guard let cur = currentSurface(dev) else {
      _ = emit([UInt8(1)])  // STATUS_UNAVAILABLE
      exit(2)
    }
    if IOSurfaceGetID(cur) != IOSurfaceGetID(surface) {
      surface = cur
    }
    let w = IOSurfaceGetWidth(surface)
    let h = IOSurfaceGetHeight(surface)

    let payload: Data
    switch op {
    case UInt8(ascii: "R"):
      payload = copyStripped(surface, width: w, height: h)
    case UInt8(ascii: "P"):
      guard let png = encodePNG(surface, width: w, height: h) else {
        _ = emit([UInt8(1)])  // encode failed → unavailable, fall back
        exit(2)
      }
      payload = png
    default:
      exit(0)  // unknown opcode → clean exit
    }

    var header: [UInt8] = [UInt8(0)]  // STATUS_OK
    header.append(contentsOf: leU32(UInt32(w)))
    header.append(contentsOf: leU32(UInt32(h)))
    header.append(contentsOf: leU32(UInt32(payload.count)))
    if !emit(header) { exit(0) }
    if !emit(payload) { exit(0) }
  }
}

// MARK: - Stream mode

guard let fps = Int(modeArg), fps >= 1, fps <= 120 else {
  fputs("invalid mode (expected FPS 1..120 or 'serve'): \(modeArg)\n", stderr)
  exit(64)
}

guard let surface = resolveSurface(udid: udidArg) else { exit(1) }

let width = IOSurfaceGetWidth(surface)
let height = IOSurfaceGetHeight(surface)
fputs("\(width)x\(height)\n", stderr)
fflush(stderr)

// MARK: - SIGINT clean exit

final class StopFlag {
  static let shared = StopFlag()
  private var raised = Int32(0)
  func raise() { OSAtomicIncrement32(&raised) }
  func isRaised() -> Bool { return raised != 0 }
}
signal(SIGINT) { _ in StopFlag.shared.raise() }
signal(SIGTERM) { _ in StopFlag.shared.raise() }

// MARK: - Main loop

let rowBytes = width * 4
let stdoutFh = FileHandle.standardOutput
let frameInterval = 1.0 / Double(fps)
let t0 = Date()
var tick = 0
while !StopFlag.shared.isRaised() {
  IOSurfaceLock(surface, [.readOnly], nil)
  let base = IOSurfaceGetBaseAddress(surface)
  let stride = IOSurfaceGetBytesPerRow(surface)
  for row in 0..<height {
    let src = base.advanced(by: row * stride)
    let data = Data(bytesNoCopy: src, count: rowBytes, deallocator: .none)
    // write() throws if stdout pipe is closed (Rust side dropped) — exit cleanly.
    do {
      try stdoutFh.write(contentsOf: data)
    } catch {
      IOSurfaceUnlock(surface, [.readOnly], nil)
      exit(0)
    }
  }
  IOSurfaceUnlock(surface, [.readOnly], nil)
  tick += 1
  let target = t0.addingTimeInterval(Double(tick) * frameInterval)
  let sleepFor = target.timeIntervalSinceNow
  if sleepFor > 0 { Thread.sleep(forTimeInterval: sleepFor) }
}
exit(0)
