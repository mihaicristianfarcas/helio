// dock DIR: the Dock's icons for the hero's Mac screen (screens.html), as
// this Mac draws them, 256 px PNGs in DIR: Finder, Safari, Terminal, System
// Settings, Helio (the installed app) and the Trash.
import AppKit

let dir = URL(fileURLWithPath: CommandLine.arguments[1])
let apps = [
    "finder": "/System/Library/CoreServices/Finder.app",
    "safari": "/Applications/Safari.app",
    "terminal": "/System/Applications/Utilities/Terminal.app",
    "settings": "/System/Applications/System Settings.app",
    "helio": "/Applications/Helio.app",
]
func save(_ image: NSImage, _ name: String) {
    let size = NSSize(width: 256, height: 256)
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 256, pixelsHigh: 256,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    image.draw(in: NSRect(origin: .zero, size: size))
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: dir.appendingPathComponent("\(name).png"))
}
for (name, path) in apps {
    guard FileManager.default.fileExists(atPath: path) else {
        FileHandle.standardError.write("no \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    // Safari is a link into the system's cryptex: its own icon, without
    // the alias arrow the link would get.
    save(NSWorkspace.shared.icon(forFile: URL(fileURLWithPath: path).resolvingSymlinksInPath().path), name)
}
save(NSImage(named: NSImage.trashEmptyName)!, "trash")
