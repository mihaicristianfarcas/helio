// bench-probe: measures a stream from the client's side on macOS, the same
// way for every stack, by watching what the Mac composites.
//
//   bench-probe click N OUT.csv       N clicks; each one's time to the flash
//   bench-probe frames SECONDS ASPECT OUT.csv
//                                     the frame counter in every composited frame
//   bench-probe display               the main display's mode, for the manifest
//   bench-probe film N OUT.csv        N clicks, each lighting the Caps Lock LED,
//                                     for a camera; nothing is captured
//
// The host runs tools/bench/scene: `flash` for `click`, a scene with the
// counter for `frames`. ASPECT is the stream's, as W:H (16:9), so the counter
// is found inside the letterbox the client draws on a 16:10 screen.
//
// How it measures, and why:
//
// - Clicks are posted at the HID level (CGEventPost, kCGHIDEventTap), where a
//   real mouse's events enter: Helio and Moonlight both read buttons from the
//   window's NSEvents, so a posted click reaches both the same way.
// - Frames come from ScreenCaptureKit, each with the time the WindowServer
//   displayed it (SCStreamFrameInfo.displayTime, mach time). That is the time
//   the frame was composited, a constant scanout and panel delay before
//   light: the same for every stack on this Mac, and measured once with a
//   camera (docs/benchmarks.md).
// - A click counts when the screen was dark when it was posted, so one
//   click's flash is never taken for the next's. The summary says why each
//   skipped click was skipped, and every captured frame's brightness goes
//   beside OUT.csv (OUT-frames.csv), so skips can be looked into: a stack
//   that skipped more would be measured on fewer, and other, clicks.
// - Capturing the screen can make macOS composite a full-screen window it
//   would otherwise hand straight to the display, and that applies equally
//   to every stack; the camera runs show what it changes.
//
// - `film` is for the direct path: nothing records the screen, so macOS
//   hands the full-screen stream straight to the display. Each click lights
//   the keyboard's Caps Lock LED at the instant it is posted, and goes out
//   with the release; a 240 fps film of the LED and the screen gives each
//   click's time to the flash (tools/bench/film-clicks). The LED's own delay
//   is the same for every stack.
//
// Needs Screen Recording and Accessibility for the program that starts it.

import CoreGraphics
import CoreMedia
import CoreVideo
import Foundation
import IOKit
import IOKit.hidsystem
import ScreenCaptureKit

setvbuf(stdout, nil, _IOLBF, 0)

var timebase = mach_timebase_info_data_t()
mach_timebase_info(&timebase)
func ns(_ mach: UInt64) -> Double { Double(mach) * Double(timebase.numer) / Double(timebase.denom) }

struct Sample {
    var display: UInt64
    var arrived: UInt64
    var value: Double  // mean brightness 0...1 (click), or the counter (frames; -1 unreadable)
}

final class Capture: NSObject, SCStreamOutput {
    let lock = NSLock()
    var samples: [Sample] = []
    let width: Int, height: Int
    let read: (UnsafePointer<UInt8>, Int, Int, Int) -> Double

    init(width: Int, height: Int, read: @escaping (UnsafePointer<UInt8>, Int, Int, Int) -> Double) {
        self.width = width
        self.height = height
        self.read = read
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
        let arrived = mach_absolute_time()
        guard type == .screen,
              let atts = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let info = atts.first,
              let raw = info[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
              let pb = CMSampleBufferGetImageBuffer(sb)
        else { return }
        let display = (info[.displayTime] as? UInt64) ?? arrived
        CVPixelBufferLockBaseAddress(pb, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pb, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(pb) else { return }
        let v = read(base.assumingMemoryBound(to: UInt8.self), CVPixelBufferGetBytesPerRow(pb),
                     CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb))
        lock.lock()
        samples.append(Sample(display: display, arrived: arrived, value: v))
        lock.unlock()
    }

    func snapshot() -> [Sample] {
        lock.lock()
        defer { lock.unlock() }
        return samples
    }
}

func stop(_ stream: SCStream) async {
    await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
        stream.stopCapture { _ in c.resume() }
    }
}

/// Mean brightness of a BGRA frame, from every fourth pixel.
func brightness(_ p: UnsafePointer<UInt8>, _ row: Int, _ w: Int, _ h: Int) -> Double {
    var sum = 0, n = 0
    for y in stride(from: 0, to: h, by: 4) {
        for x in stride(from: 0, to: w, by: 4) {
            let o = y * row + x * 4
            sum += Int(p[o]) + Int(p[o + 1]) + Int(p[o + 2])
            n += 3
        }
    }
    return n == 0 ? 0 : Double(sum) / Double(n) / 255.0
}

/// The counter of tools/bench/scene in the video rectangle `r` (fractions of
/// the frame): 16 cells in the top twentieth, their inverse below; -1 when
/// any cell agrees with its inverse (a torn or smeared frame).
func counter(_ r: CGRect) -> (UnsafePointer<UInt8>, Int, Int, Int) -> Double {
    return { p, row, w, h in
        func luma(_ fx: Double, _ fy: Double) -> Int {
            let cx = Int(fx * Double(w)), cy = Int(fy * Double(h))
            var s = 0, n = 0
            for y in max(0, cy - 1)...min(h - 1, cy + 1) {
                for x in max(0, cx - 1)...min(w - 1, cx + 1) {
                    let o = y * row + x * 4
                    s += Int(p[o]) + Int(p[o + 1]) + Int(p[o + 2])
                    n += 3
                }
            }
            return s / max(n, 1)
        }
        var value = 0
        for i in 0..<16 {
            let fx = r.minX + (Double(i) + 0.5) / 16 * r.width
            let a = luma(fx, r.minY + 0.025 * r.height) > 128
            let b = luma(fx, r.minY + 0.075 * r.height) > 128
            if a == b { return -1 }
            value = value << 1 | (a ? 1 : 0)
        }
        return Double(value)
    }
}

/// What is captured: the whole display, or with BENCH_PROBE_SOURCE=window
/// only the client's window (the largest on-screen window at the normal
/// level that covers the display, a full-screen stream's).
func filter(_ content: SCShareableContent, _ display: SCDisplay) throws -> SCContentFilter {
    guard ProcessInfo.processInfo.environment["BENCH_PROBE_SOURCE"] == "window" else {
        return SCContentFilter(display: display, excludingWindows: [])
    }
    let full = { (w: SCWindow) in w.frame.width >= CGFloat(display.width) - 1 && w.frame.height >= CGFloat(display.height) - 40 }
    let candidates = content.windows.filter { $0.isOnScreen && $0.windowLayer == 0 && full($0)
        && $0.owningApplication?.applicationName != "Window Server" }
    guard let w = candidates.max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height })
    else { throw NSError(domain: "bench-probe", code: 3, userInfo: [NSLocalizedDescriptionKey: "no full-screen window"]) }
    FileHandle.standardError.write("bench-probe: window \(w.windowID) of \(w.owningApplication?.applicationName ?? "?"), \(Int(w.frame.width))x\(Int(w.frame.height))\n".data(using: .utf8)!)
    return SCContentFilter(desktopIndependentWindow: w)
}

func start(_ capture: Capture) async throws -> SCStream {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard let display = content.displays.first(where: { $0.displayID == CGMainDisplayID() }) ?? content.displays.first
    else { throw NSError(domain: "bench-probe", code: 1, userInfo: [NSLocalizedDescriptionKey: "no display"]) }
    let cfg = SCStreamConfiguration()
    cfg.width = capture.width
    cfg.height = capture.height
    // Every frame the WindowServer composites, up to 240 a second.
    cfg.minimumFrameInterval = CMTime(value: 1, timescale: 240)
    cfg.queueDepth = 6
    cfg.pixelFormat = kCVPixelFormatType_32BGRA
    cfg.showsCursor = false
    let stream = SCStream(filter: try filter(content, display), configuration: cfg, delegate: nil)
    try stream.addStreamOutput(capture, type: .screen, sampleHandlerQueue: DispatchQueue(label: "bench-probe.capture", qos: .userInteractive))
    try await stream.startCapture()
    return stream
}

func displayAspect() -> Double {
    let b = CGDisplayBounds(CGMainDisplayID())
    return b.width / b.height
}

func post(_ type: CGEventType) -> UInt64 {
    let src = CGEventSource(stateID: .hidSystemState)
    let at = CGEvent(source: nil)?.location ?? .zero
    let e = CGEvent(mouseEventSource: src, mouseType: type, mouseCursorPosition: at, mouseButton: .left)
    let t = mach_absolute_time()
    e?.post(tap: .cghidEventTap)
    return t
}

func median(_ xs: [Double]) -> Double {
    let s = xs.sorted()
    return s.isEmpty ? .nan : (s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2)
}

func pct(_ xs: [Double], _ q: Double) -> Double {
    let s = xs.sorted()
    return s.isEmpty ? .nan : s[min(s.count - 1, Int(Double(s.count - 1) * q + 0.5))]
}

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write(("bench-probe: " + msg + "\n").data(using: .utf8)!)
    exit(2)
}

let args = CommandLine.arguments
guard args.count >= 2 else { fail("usage: bench-probe click N OUT.csv | frames SECONDS W:H OUT.csv | display") }

switch args[1] {
case "click":
    guard args.count == 4, let n = Int(args[2]) else { fail("usage: bench-probe click N OUT.csv") }
    let aspect = displayAspect()
    let capture = Capture(width: 96, height: Int(96 / aspect), read: brightness)
    let stream = try await start(capture)
    try await Task.sleep(nanoseconds: 1_000_000_000)
    var clicks: [(down: UInt64, up: UInt64)] = []
    for _ in 0..<n {
        // A random gap so clicks fall at every phase of the host's and the
        // client's refresh.
        try await Task.sleep(nanoseconds: UInt64.random(in: 450_000_000...850_000_000))
        let down = post(.leftMouseDown)
        try await Task.sleep(nanoseconds: 120_000_000)
        let up = post(.leftMouseUp)
        clicks.append((down, up))
    }
    try await Task.sleep(nanoseconds: 600_000_000)
    await stop(stream)
    let frames = capture.snapshot()
    var out = "click,down_ns,flash_ns,latency_ms,arrival_ms\n"
    var lat: [Double] = []
    var skipped = 0
    // Why clicks were skipped: no frame captured before the click, the last
    // one before it lit, no lit frame after it, the first lit one too late.
    var (noFrame, notDark, noFlash, late) = (0, 0, 0, 0)
    for (i, c) in clicks.enumerated() {
        guard let before = frames.last(where: { $0.display <= c.down }) else { noFrame += 1; skipped += 1; continue }
        guard before.value < 0.5 else { notDark += 1; skipped += 1; continue }
        guard let lit = frames.first(where: { $0.display > c.down && $0.value >= 0.5 })
        else { noFlash += 1; skipped += 1; continue }
        let ms = (ns(lit.display) - ns(c.down)) / 1e6
        // Past 400 ms it is not this click's flash.
        guard ms < 400 else { late += 1; skipped += 1; continue }
        let arrival = (ns(lit.arrived) - ns(c.down)) / 1e6
        lat.append(ms)
        out += "\(i),\(Int(ns(c.down))),\(Int(ns(lit.display))),\(String(format: "%.3f", ms)),\(String(format: "%.3f", arrival))\n"
    }
    try out.write(toFile: args[3], atomically: true, encoding: .utf8)
    var all = "display_ns,arrived_ns,brightness\n"
    for f in frames { all += "\(Int(ns(f.display))),\(Int(ns(f.arrived))),\(String(format: "%.3f", f.value))\n" }
    let side = args[3].hasSuffix(".csv") ? String(args[3].dropLast(4)) + "-frames.csv" : args[3] + "-frames.csv"
    try all.write(toFile: side, atomically: true, encoding: .utf8)
    print(String(format: "clicks %d measured %d skipped %d frames %d median %.2f p95 %.2f min %.2f max %.2f ms",
                 n, lat.count, skipped, frames.count, median(lat), pct(lat, 0.95), lat.min() ?? .nan, lat.max() ?? .nan))
    print("skipped: no frame before \(noFrame), lit before \(notDark), no flash after \(noFlash), flash after 400 ms \(late)")

case "frames":
    guard args.count == 5, let secs = Double(args[2]) else { fail("usage: bench-probe frames SECONDS W:H OUT.csv") }
    let parts = args[3].split(separator: ":").compactMap { Double($0) }
    guard parts.count == 2 else { fail("aspect is W:H") }
    let video = parts[0] / parts[1], screen = displayAspect()
    // The client fits the video inside the screen, centred.
    let rect = video >= screen
        ? CGRect(x: 0, y: (1 - screen / video) / 2, width: 1, height: screen / video)
        : CGRect(x: (1 - video / screen) / 2, y: 0, width: video / screen, height: 1)
    let capture = Capture(width: 768, height: Int(768 / screen), read: counter(rect))
    let stream = try await start(capture)
    try await Task.sleep(nanoseconds: UInt64(secs * 1e9))
    await stop(stream)
    let frames = capture.snapshot()
    var out = "display_ns,arrived_ns,counter\n"
    for f in frames { out += "\(Int(ns(f.display))),\(Int(ns(f.arrived))),\(Int(f.value))\n" }
    try out.write(toFile: args[4], atomically: true, encoding: .utf8)
    let read = frames.filter { $0.value >= 0 }
    print("frames \(frames.count) readable \(read.count) distinct \(Set(read.map { Int($0.value) }).count)")

case "film":
    guard args.count == 4, let n = Int(args[2]) else { fail("usage: bench-probe film N OUT.csv") }
    // The HID system's lock state: setting Caps Lock lights its LED, and
    // sends no key to the client.
    let service = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching(kIOHIDSystemClass))
    var connect: io_connect_t = 0
    guard service != 0, IOServiceOpen(service, mach_task_self_, UInt32(kIOHIDParamConnectType), &connect) == KERN_SUCCESS
    else { fail("cannot open the HID system for the Caps Lock LED") }
    func caps(_ on: Bool) { _ = IOHIDSetModifierLockState(connect, Int32(kIOHIDCapsLockState), on) }
    caps(false)
    // Time to start the camera and settle the screen.
    try await Task.sleep(nanoseconds: 3_000_000_000)
    var out = "click,down_ns,up_ns\n"
    for i in 0..<n {
        // Long, random gaps: the screen is dark again before each click, and
        // clicks fall at every phase of both refreshes.
        try await Task.sleep(nanoseconds: UInt64.random(in: 600_000_000...1_000_000_000))
        caps(true)
        let down = post(.leftMouseDown)
        try await Task.sleep(nanoseconds: 150_000_000)
        caps(false)
        let up = post(.leftMouseUp)
        out += "\(i),\(Int(ns(down))),\(Int(ns(up)))\n"
    }
    caps(false)
    IOServiceClose(connect)
    try out.write(toFile: args[3], atomically: true, encoding: .utf8)
    print("film: \(n) clicks")

case "display":
    // The main display's mode, for the run's manifest: points, the pixels
    // they are drawn at, and the refresh rate.
    guard let m = CGDisplayCopyDisplayMode(CGMainDisplayID()) else { fail("no display mode") }
    print("client_display=\(m.width)x\(m.height) points, drawn at \(m.pixelWidth)x\(m.pixelHeight), \(Int(m.refreshRate.rounded())) Hz")

default:
    fail("unknown mode \(args[1])")
}
