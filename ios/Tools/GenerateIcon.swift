import AppKit

let size = 1024
let output = CommandLine.arguments.dropFirst().first ?? "ios/MasterKey/Assets.xcassets/AppIcon.appiconset/AppIcon.png"

guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: size,
    pixelsHigh: size,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
) else { fatalError("Could not create the icon bitmap.") }

NSGraphicsContext.saveGraphicsState()
let graphics = NSGraphicsContext(bitmapImageRep: bitmap)!
NSGraphicsContext.current = graphics
graphics.imageInterpolation = .high

let background = NSColor(red: 16 / 255, green: 28 / 255, blue: 40 / 255, alpha: 1)
let mint = NSColor(red: 164 / 255, green: 223 / 255, blue: 204 / 255, alpha: 1)
background.setFill()
NSBezierPath(rect: NSRect(x: 0, y: 0, width: size, height: size)).fill()

// Keep the shaft tucked under the bow so the mark reads as one solid key.
let shaft = NSBezierPath()
shaft.move(to: NSPoint(x: 535, y: 784))
shaft.line(to: NSPoint(x: 756, y: 564))
shaft.line(to: NSPoint(x: 848, y: 656))
shaft.line(to: NSPoint(x: 793, y: 711))
shaft.line(to: NSPoint(x: 852, y: 770))
shaft.line(to: NSPoint(x: 783, y: 839))
shaft.line(to: NSPoint(x: 724, y: 780))
shaft.line(to: NSPoint(x: 669, y: 835))
shaft.line(to: NSPoint(x: 578, y: 744))
shaft.close()
mint.setFill()
shaft.fill()

let bow = NSBezierPath(ovalIn: NSRect(x: 184, y: 412, width: 456, height: 456))
bow.appendOval(in: NSRect(x: 304, y: 532, width: 216, height: 216))
bow.windingRule = .evenOdd
mint.setFill()
bow.fill()

graphics.flushGraphics()
NSGraphicsContext.restoreGraphicsState()

let url = URL(fileURLWithPath: output)
try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
guard let png = bitmap.representation(using: .png, properties: [:]) else { fatalError("Could not encode the icon PNG.") }
try png.write(to: url)
print("Wrote 1024×1024 app icon to \(url.path)")
