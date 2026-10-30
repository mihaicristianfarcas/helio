// A picture as a borderless window behind Helio: the Mac's glass then blurs
// that picture, and this Mac's own wallpaper and windows stay out of it. The
// pointer is parked on it, so no row in the capture looks hovered.
// Usage: wall IMAGE X Y W H SECONDS (points, from the top left).
import AppKit
let a = CommandLine.arguments
let image = NSImage(contentsOfFile: a[1])!
let (x, y, w, h) = (Double(a[2])!, Double(a[3])!, Double(a[4])!, Double(a[5])!)
let screen = NSScreen.main!.frame
CGWarpMouseCursorPosition(CGPoint(x: x + 6, y: y + h - 6))
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
// A borderless window can take the front only as the key window.
class Wall: NSWindow { override var canBecomeKey: Bool { true } }
let window = Wall(contentRect: NSRect(x: x, y: screen.height - y - h, width: w, height: h),
                      styleMask: .borderless, backing: .buffered, defer: false)
let view = NSImageView(frame: NSRect(x: 0, y: 0, width: w, height: h))
view.image = image
view.imageScaling = .scaleProportionallyUpOrDown
// Cover, not fit: the picture is drawn larger than the window, centred.
let scale = max(w / image.size.width, h / image.size.height)
view.frame = NSRect(x: (w - image.size.width * scale) / 2, y: (h - image.size.height * scale) / 2,
                    width: image.size.width * scale, height: image.size.height * scale)
let holder = NSView(frame: NSRect(x: 0, y: 0, width: w, height: h))
holder.wantsLayer = true
holder.addSubview(view)
window.contentView = holder
window.isOpaque = true
window.hasShadow = false
app.activate(ignoringOtherApps: true)
window.makeKeyAndOrderFront(nil)
DispatchQueue.main.asyncAfter(deadline: .now() + Double(a[6])!) { exit(0) }
app.run()
