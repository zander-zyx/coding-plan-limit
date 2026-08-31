// 圆角图标工具：居中裁方 + 圆角 + 可选 macOS 规范边距（CoreGraphics，macOS 自带 swift 运行）
// 用法：swift scripts/round-icon.swift <输入图> <输出PNG> [输出尺寸] [圆角比例] [边距比例]
// 边距比例默认 0（满铺）；0.1 = 四周各留 10%，主体 824/1024，与 Apple 图标网格一致，
// 保证 Dock 里与其他应用图标视觉等大。
import AppKit

let a = CommandLine.arguments
guard a.count >= 3 else {
    fputs("usage: swift round-icon.swift <in> <out.png> [size] [ratio] [margin]\n", stderr)
    exit(1)
}
let out = a.count > 3 ? Int(a[3]) ?? 1024 : 1024
let ratio = a.count > 4 ? (Double(a[4]) ?? 0.227) : 0.227
let margin = a.count > 5 ? (Double(a[5]) ?? 0) : 0

guard let src = NSImage(contentsOfFile: a[1])?.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    fputs("cannot load \(a[1])\n", stderr); exit(2)
}
let side = min(src.width, src.height)
let crop = CGRect(x: (src.width - side) / 2, y: (src.height - side) / 2, width: side, height: side)
guard let cropped = src.cropping(to: crop) else { fputs("crop fail\n", stderr); exit(3) }

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: out, pixelsHigh: out,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
else { fputs("rep fail\n", stderr); exit(4) }

let ctx = NSGraphicsContext(bitmapImageRep: rep)!.cgContext
let inset = CGFloat(Double(out) * margin)
let body = CGRect(x: inset, y: inset, width: CGFloat(out) - inset * 2, height: CGFloat(out) - inset * 2)
let r = body.width * CGFloat(ratio)
ctx.addPath(CGPath(roundedRect: body, cornerWidth: r, cornerHeight: r, transform: nil))
ctx.clip()
ctx.interpolationQuality = .high
ctx.draw(cropped, in: body)

guard let png = rep.representation(using: .png, properties: [:]) else { fputs("encode fail\n", stderr); exit(5) }
try png.write(to: URL(fileURLWithPath: a[2]))
print("OK \(out)x\(out) r=\(ratio) margin=\(margin)")
