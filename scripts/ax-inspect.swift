#!/usr/bin/env swift

import Cocoa
import CoreGraphics

// Get the frontmost application
guard let frontApp = NSWorkspace.shared.frontmostApplication else {
    print("No frontmost application")
    exit(1)
}

let pid = frontApp.processIdentifier
let appElement = AXUIElementCreateApplication(pid)

// Get the focused window
var focusedWindow: CFTypeRef?
AXUIElementCopyAttributeValue(appElement, kAXFocusedWindowAttribute as CFString, &focusedWindow)

guard let window = focusedWindow else {
    print("No focused window")
    exit(1)
}

// Query AX attributes
func getAttribute(_ element: AXUIElement, _ attribute: String) -> String? {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
    if result == .success, let str = value as? String {
        return str
    }
    return nil
}

// Get window position for matching with CGWindowList
func getWindowPosition(_ element: AXUIElement) -> CGPoint? {
    var positionRef: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, kAXPositionAttribute as CFString, &positionRef)
    if result == .success, let positionRef = positionRef {
        var position = CGPoint.zero
        AXValueGetValue(positionRef as! AXValue, .cgPoint, &position)
        return position
    }
    return nil
}

// Get window level from CGWindowList by matching position
func getWindowLevel(pid: pid_t, position: CGPoint) -> Int? {
    let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
    guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
        return nil
    }

    for windowDict in windowList {
        guard let windowPid = windowDict[kCGWindowOwnerPID as String] as? pid_t,
              windowPid == pid,
              let boundsDict = windowDict[kCGWindowBounds as String] as? [String: CGFloat] else {
            continue
        }

        let x = boundsDict["X"] ?? 0
        let y = boundsDict["Y"] ?? 0

        // Match by position (allow small tolerance)
        if abs(x - position.x) < 2 && abs(y - position.y) < 2 {
            return windowDict[kCGWindowLayer as String] as? Int
        }
    }
    return nil
}

let windowElement = window as! AXUIElement

// Get window level
var windowLevelStr = "(unknown)"
if let position = getWindowPosition(windowElement),
   let level = getWindowLevel(pid: pid, position: position) {
    // Map common levels to names
    switch level {
    case 0: windowLevelStr = "0 (normal)"
    case 3: windowLevelStr = "3 (floating)"
    case 8: windowLevelStr = "8 (modal)"
    case 19: windowLevelStr = "19 (utility)"
    case 101: windowLevelStr = "101 (popup)"
    default: windowLevelStr = "\(level)"
    }
}

print("Application: \(frontApp.localizedName ?? "Unknown")")
print("Bundle ID: \(frontApp.bundleIdentifier ?? "Unknown")")
print("Window Level: \(windowLevelStr)")
print("AXIdentifier: \(getAttribute(windowElement, "AXIdentifier") ?? "(none)")")
print("AXSubrole: \(getAttribute(windowElement, "AXSubrole") ?? "(none)")")
print("AXTitle: \(getAttribute(windowElement, "AXTitle") ?? "(none)")")
