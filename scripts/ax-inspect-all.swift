#!/usr/bin/env swift

import Cocoa
import CoreGraphics

// Query AX attribute from window element
func getAttribute(_ element: AXUIElement, _ attribute: String) -> String? {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
    if result == .success, let str = value as? String {
        return str
    }
    return nil
}

// Get AX window element by matching position
func getAXWindow(appElement: AXUIElement, targetBounds: CGRect) -> AXUIElement? {
    var windowsRef: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(appElement, kAXWindowsAttribute as CFString, &windowsRef)
    guard result == .success, let windows = windowsRef as? [AXUIElement] else {
        return nil
    }

    for window in windows {
        var positionRef: CFTypeRef?
        var sizeRef: CFTypeRef?

        AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &positionRef)
        AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeRef)

        if let positionRef = positionRef, let sizeRef = sizeRef {
            var position = CGPoint.zero
            var size = CGSize.zero
            AXValueGetValue(positionRef as! AXValue, .cgPoint, &position)
            AXValueGetValue(sizeRef as! AXValue, .cgSize, &size)

            // Match by position (allow small tolerance)
            if abs(position.x - targetBounds.origin.x) < 2 &&
               abs(position.y - targetBounds.origin.y) < 2 {
                return window
            }
        }
    }
    return nil
}

// Get all on-screen windows
let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    print("Failed to get window list")
    exit(1)
}

// Group windows by application
struct WindowInfo {
    let windowId: Int
    let bounds: CGRect
    let layer: Int
    let title: String?
    let axIdentifier: String?
    let axSubrole: String?
    let axTitle: String?
}

struct AppInfo {
    let name: String
    let bundleId: String?
    let pid: pid_t
    var windows: [WindowInfo]
}

var apps: [pid_t: AppInfo] = [:]

for windowDict in windowList {
    guard let pid = windowDict[kCGWindowOwnerPID as String] as? pid_t,
          let windowId = windowDict[kCGWindowNumber as String] as? Int,
          let boundsDict = windowDict[kCGWindowBounds as String] as? [String: CGFloat] else {
        continue
    }

    let bounds = CGRect(
        x: boundsDict["X"] ?? 0,
        y: boundsDict["Y"] ?? 0,
        width: boundsDict["Width"] ?? 0,
        height: boundsDict["Height"] ?? 0
    )

    let layer = windowDict[kCGWindowLayer as String] as? Int ?? 0
    let title = windowDict[kCGWindowName as String] as? String
    let appName = windowDict[kCGWindowOwnerName as String] as? String ?? "Unknown"

    // Get bundle ID from running application
    let runningApps = NSWorkspace.shared.runningApplications.filter { $0.processIdentifier == pid }
    let bundleId = runningApps.first?.bundleIdentifier

    // Query AX attributes
    let appElement = AXUIElementCreateApplication(pid)
    var axIdentifier: String? = nil
    var axSubrole: String? = nil
    var axTitle: String? = nil

    if let axWindow = getAXWindow(appElement: appElement, targetBounds: bounds) {
        axIdentifier = getAttribute(axWindow, "AXIdentifier")
        axSubrole = getAttribute(axWindow, "AXSubrole")
        axTitle = getAttribute(axWindow, "AXTitle")
    }

    let windowInfo = WindowInfo(
        windowId: windowId,
        bounds: bounds,
        layer: layer,
        title: title,
        axIdentifier: axIdentifier,
        axSubrole: axSubrole,
        axTitle: axTitle
    )

    if var appInfo = apps[pid] {
        appInfo.windows.append(windowInfo)
        apps[pid] = appInfo
    } else {
        apps[pid] = AppInfo(
            name: appName,
            bundleId: bundleId,
            pid: pid,
            windows: [windowInfo]
        )
    }
}

// Print results grouped by application
let sortedApps = apps.values.sorted { $0.name.lowercased() < $1.name.lowercased() }

for app in sortedApps {
    print("=== \(app.name) ===")
    print("  Bundle ID: \(app.bundleId ?? "(none)")")
    print("  PID: \(app.pid)")
    print("")

    for window in app.windows {
        print("  Window ID: \(window.windowId)")
        print("    Layer: \(window.layer)")
        print("    Bounds: \(Int(window.bounds.origin.x)),\(Int(window.bounds.origin.y)) \(Int(window.bounds.width))x\(Int(window.bounds.height))")
        if let title = window.title ?? window.axTitle {
            print("    Title: \(title)")
        }
        print("    AXIdentifier: \(window.axIdentifier ?? "(none)")")
        print("    AXSubrole: \(window.axSubrole ?? "(none)")")
        print("")
    }
}
