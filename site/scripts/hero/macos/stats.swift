// stats TEXT OUT: the stream's statistics as Helio draws them over a stream on
// a Mac (helio-client/src/mac/text.rs, rasterize), at 2x: each line of TEXT
// in Menlo 13 white on black at 62%, 10 points of padding, 3 between lines.
// A PNG of that panel alone, to lay over another picture of the stream.
import AppKit
import CoreText

let text = try! String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8)
let out = URL(fileURLWithPath: CommandLine.arguments[2])
let scale: CGFloat = 2
let font = CTFontCreateWithName("Menlo" as CFString, 13 * scale, nil)
let pad = 10 * scale, gap = 3 * scale
let attrs: [CFString: Any] = [kCTFontAttributeName: font, kCTForegroundColorAttributeName: CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 1)]
var lines: [(CTLine, CGFloat, CGFloat, CGFloat)] = []
for s in text.split(separator: "\n", omittingEmptySubsequences: false) where !s.isEmpty {
    let line = CTLineCreateWithAttributedString(CFAttributedStringCreate(nil, String(s) as CFString, attrs as CFDictionary))
    var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
    let width = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, &leading))
    lines.append((line, width, ascent, descent))
}
let textW = lines.map { $0.1 }.max()!
let lineH = lines.map { $0.2 + $0.3 }.max()! + gap
let width = Int((textW + 2 * pad).rounded(.up)), height = Int((lineH * CGFloat(lines.count) + 2 * pad).rounded(.up))
let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                    space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.setFillColor(red: 0, green: 0, blue: 0, alpha: 0.62)
ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
var y = CGFloat(height) - pad
for (line, _, ascent, descent) in lines {
    y -= ascent
    ctx.textPosition = CGPoint(x: pad, y: y)
    CTLineDraw(line, ctx)
    y -= descent + gap
}
let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
CGImageDestinationFinalize(dest)
print(width, height)
