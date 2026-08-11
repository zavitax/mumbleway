// Post a real mouse click at a global screen point.
//
//   swift tool/click.swift 280 503          # click
//   swift tool/click.swift 1400 900 move    # move only
//
// `System Events -> click at {x, y}` does NOT work on this app. It resolves the
// element under the point and sends it an AXPress; Flutter draws the whole
// interface into one NSView, so the only thing there is a generic group.
// Pressing it does nothing, the call still succeeds, and it returns
// "group 1 of window mumbleway" — which reads exactly like a click that worked.
//
// A CGEvent is a real HID-level event, so it lands wherever the pointer is put
// regardless of what the accessibility tree exposes. Needs Accessibility
// permission, the same grant System Events needs for moving and sizing windows.

import CoreGraphics
import Foundation

let a = CommandLine.arguments
guard a.count >= 3, let x = Double(a[1]), let y = Double(a[2]) else {
    FileHandle.standardError.write("usage: click.swift <x> <y> [move]\n".data(using: .utf8)!)
    exit(2)
}
let p = CGPoint(x: x, y: y)

// Move first, always. Some controls only accept a click after the pointer has
// entered them, and a click posted at a point the pointer never visited can be
// delivered to whatever it was hovering over before.
//
// The move is also how a shot is kept clean: park the pointer off the window
// before capturing, or a tooltip is in the picture.
CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)?
    .post(tap: .cghidEventTap)
usleep(120_000)

if a.count > 3 && a[3] == "move" { exit(0) }

CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)?
    .post(tap: .cghidEventTap)
usleep(90_000)
CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)?
    .post(tap: .cghidEventTap)
