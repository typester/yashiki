#!/usr/bin/env swift

import Cocoa

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

// Query attributes
func getAttribute(_ element: AXUIElement, _ attribute: String) -> String? {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
    if result == .success, let str = value as? String {
        return str
    }
    return nil
}

let windowElement = window as! AXUIElement

print("Application: \(frontApp.localizedName ?? "Unknown")")
print("Bundle ID: \(frontApp.bundleIdentifier ?? "Unknown")")
print("AXIdentifier: \(getAttribute(windowElement, "AXIdentifier") ?? "(none)")")
print("AXSubrole: \(getAttribute(windowElement, "AXSubrole") ?? "(none)")")
print("AXTitle: \(getAttribute(windowElement, "AXTitle") ?? "(none)")")
