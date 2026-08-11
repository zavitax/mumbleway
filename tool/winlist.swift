// Lists on-screen windows with their ids and bounds, for `screencapture -l`.
//
//   swift tool/winlist.swift
//   → 796	mumbleway	80,80 1000x720	mumbleway
//
// `CGWindowListCopyWindowInfo` reads geometry rather than pixels, so this needs
// no Screen Recording permission — only `screencapture` itself does. That split
// is worth knowing: this will happily list windows on a machine where the
// capture is still refused.
//
// One caveat that reads as a bug: without Screen Recording granted, every
// `kCGWindowName` comes back empty. Titles appearing is the signal that the
// grant went through.

import CoreGraphics
import Foundation

let opts = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
guard let infos = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else {
    FileHandle.standardError.write("could not read the window list\n".data(using: .utf8)!)
    exit(1)
}

for w in infos {
    let owner = w[kCGWindowOwnerName as String] as? String ?? "?"
    let num = w[kCGWindowNumber as String] as? Int ?? -1
    let name = w[kCGWindowName as String] as? String ?? ""
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let x = b["X"] as? Double ?? 0, y = b["Y"] as? Double ?? 0
    let ww = b["Width"] as? Double ?? 0, hh = b["Height"] as? Double ?? 0
    // Anything smaller than this is a shadow, a tooltip or a menu-bar item.
    if ww > 200 && hh > 200 {
        print("\(num)\t\(owner)\t\(Int(x)),\(Int(y)) \(Int(ww))x\(Int(hh))\t\(name)")
    }
}
