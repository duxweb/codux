#!/usr/bin/env swift

import AppKit
import Foundation

func renderIcon(size: CGFloat) -> NSImage {
    let image = NSImage(size: NSSize(width: size, height: size))
    image.lockFocus()
    defer { image.unlockFocus() }

    let inset = size * 0.08
    let rect = NSRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
    let radius = size * 0.22
    let shape = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)

    let top = NSColor(calibratedRed: 0.24, green: 0.50, blue: 0.98, alpha: 1)
    let bottom = NSColor(calibratedRed: 0.16, green: 0.36, blue: 0.86, alpha: 1)

    NSGraphicsContext.saveGraphicsState()
    shape.addClip()

    let bg = NSGradient(starting: top, ending: bottom)!
    bg.draw(in: rect, angle: 90)

    let topCenter = CGPoint(x: rect.midX, y: rect.maxY - size * 0.08)
    if let topGlow = NSGradient(
        colors: [NSColor.white.withAlphaComponent(0.10), NSColor.white.withAlphaComponent(0.0)],
        atLocations: [0.0, 1.0],
        colorSpace: .deviceRGB
    ) {
        topGlow.draw(fromCenter: topCenter, radius: 0, toCenter: topCenter, radius: size * 0.50, options: [.drawsAfterEndingLocation])
    }

    let bottomCenter = CGPoint(x: rect.midX, y: rect.minY)
    if let bottomShade = NSGradient(
        colors: [NSColor.black.withAlphaComponent(0.08), NSColor.black.withAlphaComponent(0.0)],
        atLocations: [0.0, 1.0],
        colorSpace: .deviceRGB
    ) {
        bottomShade.draw(fromCenter: bottomCenter, radius: 0, toCenter: bottomCenter, radius: size * 0.45, options: [.drawsAfterEndingLocation])
    }

    let cx = rect.midX
    let cy = rect.midY
    let chevronH = size * 0.30
    let chevronW = size * 0.17
    let weight = size * 0.09

    let backOffsetX = size * -0.10
    let backChevron = NSBezierPath()
    backChevron.move(to: CGPoint(x: cx + backOffsetX - chevronW * 0.5, y: cy + chevronH * 0.5))
    backChevron.line(to: CGPoint(x: cx + backOffsetX + chevronW * 0.5, y: cy))
    backChevron.line(to: CGPoint(x: cx + backOffsetX - chevronW * 0.5, y: cy - chevronH * 0.5))
    NSColor.white.withAlphaComponent(0.4).setStroke()
    backChevron.lineWidth = weight
    backChevron.lineCapStyle = .square
    backChevron.lineJoinStyle = .miter
    backChevron.stroke()

    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
    shadow.shadowOffset = NSSize(width: 0, height: -size * 0.01)
    shadow.shadowBlurRadius = size * 0.02
    shadow.set()

    let frontOffsetX = size * 0.10
    let frontChevron = NSBezierPath()
    frontChevron.move(to: CGPoint(x: cx + frontOffsetX - chevronW * 0.5, y: cy + chevronH * 0.5))
    frontChevron.line(to: CGPoint(x: cx + frontOffsetX + chevronW * 0.5, y: cy))
    frontChevron.line(to: CGPoint(x: cx + frontOffsetX - chevronW * 0.5, y: cy - chevronH * 0.5))
    NSColor.white.setStroke()
    frontChevron.lineWidth = weight
    frontChevron.lineCapStyle = .square
    frontChevron.lineJoinStyle = .miter
    frontChevron.stroke()

    let clearShadow = NSShadow()
    clearShadow.shadowColor = nil
    clearShadow.set()

    let innerBorder = NSBezierPath(roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)
    NSColor.white.withAlphaComponent(0.08).setStroke()
    innerBorder.lineWidth = 1.0
    innerBorder.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return image
}

func writePNG(to url: URL, size: CGFloat) throws {
    let image = renderIcon(size: size)
    guard let tiffData = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiffData),
          let png = bitmap.representation(using: .png, properties: [:]) else {
        throw NSError(domain: "generate-app-icon", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to encode PNG"])
    }
    try png.write(to: url)
}

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
    fputs("usage: generate-app-icon.swift <iconset-dir>\n", stderr)
    exit(1)
}

let outputDirectory = URL(fileURLWithPath: arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: outputDirectory, withIntermediateDirectories: true)

let entries: [(String, CGFloat)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

for (name, size) in entries {
    try writePNG(to: outputDirectory.appendingPathComponent(name), size: size)
}
