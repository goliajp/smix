// smix-capture-host — simulator framebuffer streamer.
//
// Reads CoreSimulator's display IOSurface for a booted sim and streams BGRA
// frames at a target FPS:
//
//   argv : <UDID> <FPS>
//   stderr: "<W>x<H>\n" geometry header (one line), then any error text
//   stdout: contiguous BGRA frames, exactly W*H*4 bytes each, row padding stripped
//
// Apple framework only (Foundation + IOSurface). All CoreSimulator entry
// points are resolved via dlopen + objc dynamic lookup, so there is no
// link-time hard dependency on private frameworks.
//
// SIGINT triggers a clean exit (atomic flag, drains current iteration).

import Foundation
import IOSurface

// MARK: - Argv

let argv = CommandLine.arguments
guard argv.count >= 3 else {
  fputs("usage: smix-capture-host <UDID> <FPS>\n", stderr)
  exit(64)
}
let udidArg = argv[1].uppercased()
guard let fps = Int(argv[2]), fps >= 1, fps <= 120 else {
  fputs("invalid FPS (1..120): \(argv[2])\n", stderr)
  exit(64)
}

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

func resolveSurface(udid: String) -> IOSurface? {
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
    }),
    let io = dev.value(forKey: "io") as? NSObject,
    let ports = io.value(forKey: "ioPorts") as? [NSObject]
  else {
    fputs("device/io resolution failed for udid=\(udid)\n", stderr)
    return nil
  }
  for port in ports {
    guard let d = call(port, "descriptor") else { continue }
    if let s = call(d, "framebufferSurface") {
      return unsafeDowncast(s, to: IOSurface.self)
    }
  }
  fputs("no port with a non-nil framebufferSurface\n", stderr)
  return nil
}

guard let surface = resolveSurface(udid: udidArg) else { exit(1) }

// MARK: - Header

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
