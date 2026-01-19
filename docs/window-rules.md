# Window Rules Guide

## Table of Contents

- [Overview](#overview)
- [Matching Options](#matching-options)
  - [Window Level Matcher](#window-level-matcher)
  - [Button State Matchers](#button-state-matchers)
  - [Combining Matchers](#combining-matchers)
- [Available Actions](#available-actions)
  - [ignore vs float](#ignore-vs-float)
- [Rule Specificity](#rule-specificity)
- [Managing Rules](#managing-rules)
- [Finding AX Attributes](#finding-ax-attributes)
  - [Using Accessibility Inspector](#using-accessibility-inspector)
  - [Using Terminal](#using-terminal)
  - [Debugging Window Discovery](#debugging-window-discovery)
- [Common Use Cases](#common-use-cases)
  - [Floating Dialog Windows](#floating-dialog-windows)
  - [Quick/Popup Windows](#quickpopup-windows)
  - [Assigning Windows to Tags](#assigning-windows-to-tags)
  - [Moving Windows to Specific Displays](#moving-windows-to-specific-displays)
- [Troubleshooting](#troubleshooting)

## Overview

Window rules let you automatically configure window properties based on various attributes. Rules are applied when windows are created, allowing you to:

- Ignore specific windows completely (never manage)
- Float specific windows (exclude from tiling)
- Assign windows to specific tags
- Move windows to specific displays
- Set initial position and dimensions

## Matching Options

| Option | Description | Example |
|--------|-------------|---------|
| `--app-name` | Application name | `Safari`, `*Chrome*` |
| `--app-id` | Bundle identifier | `com.apple.Safari`, `com.google.*` |
| `--title` | Window title | `*Preferences*`, `*Dialog*` |
| `--ax-id` | AXIdentifier attribute | `com.mitchellh.ghostty.quickTerminal` |
| `--subrole` | AXSubrole attribute | `Dialog`, `FloatingWindow` |
| `--window-level` | Window level | `normal`, `floating`, `other`, `8` |
| `--close-button` | Close button state | `exists`, `none`, `enabled`, `disabled` |
| `--fullscreen-button` | Fullscreen button state | `exists`, `none`, `enabled`, `disabled` |
| `--minimize-button` | Minimize button state | `exists`, `none`, `enabled`, `disabled` |
| `--zoom-button` | Zoom button state | `exists`, `none`, `enabled`, `disabled` |

Glob patterns (`*` for any characters) are supported for `--app-name`, `--app-id`, `--title`, `--ax-id`, and `--subrole`.

### Window Level Matcher

The `--window-level` option matches windows based on their CGWindowLevel:

| Value | Level | Description |
|-------|-------|-------------|
| `normal` | 0 | Standard application windows |
| `floating` | 3 | Utility panels, palettes |
| `modal` | 8 | Modal dialogs |
| `utility` | 19 | Utility windows |
| `popup` | 101 | Popup menus |
| `other` | != 0 | Any non-normal window |
| `<number>` | N | Specific numeric level |

```sh
# Ignore all non-normal windows (palettes, panels, etc.)
yashiki rule-add --window-level other ignore

# Float utility panels (level 3)
yashiki rule-add --window-level floating float

# Match by specific numeric level
yashiki rule-add --window-level 8 float
```

### Button State Matchers

Button matchers check the state of window control buttons:

| Value | Description |
|-------|-------------|
| `exists` | Button exists (enabled or disabled) |
| `none` | Button doesn't exist |
| `enabled` | Button exists and is enabled |
| `disabled` | Button exists but is disabled |

```sh
# Float windows without fullscreen button (dialogs, sheets)
yashiki rule-add --fullscreen-button none float

# Ignore windows without close button (popups, tooltips)
yashiki rule-add --close-button none ignore

# Ghostty Quick Terminal: fullscreen disabled, close enabled
yashiki rule-add --app-id com.mitchellh.ghostty --fullscreen-button disabled ignore

# Firefox PiP: minimize button is disabled
yashiki rule-add --app-id org.mozilla.firefox --minimize-button disabled float
```

### Combining Matchers

You can combine multiple matchers for more specific rules:

```sh
# Safari preferences window
yashiki rule-add --app-name Safari --title "*Preferences*" float

# Ghostty floating windows
yashiki rule-add --app-id com.mitchellh.ghostty --subrole FloatingWindow float

# Ghostty Quick Terminal (specific combination)
yashiki rule-add --app-id com.mitchellh.ghostty --fullscreen-button disabled --close-button enabled ignore
```

## Available Actions

| Action | Syntax | Description |
|--------|--------|-------------|
| `ignore` | `ignore` | Never manage (skip completely) |
| `float` | `float` | Window floats (excluded from tiling) |
| `no-float` | `no-float` | Override more general float rules |
| `tags` | `tags <bitmask>` | Set window tags |
| `output` | `output <id\|name>` | Move to specific display |
| `position` | `position <x> <y>` | Set initial position |
| `dimensions` | `dimensions <w> <h>` | Set initial size |

### ignore vs float

Both `ignore` and `float` exclude windows from tiling, but they behave differently.

|  | `ignore` | `float` |
|--|----------|---------|
| **Managed** | No (completely skipped) | Yes (floating state) |
| **list-windows** | Not shown | Shown |
| **window-focus** | Cannot focus | Can focus |
| **Tag operations** | No effect | Hidden/shown on tag switch |
| **Manual toggle** | Not possible | `window-toggle-float` works |
| **Use case** | Temporary windows (popups, tooltips, dropdowns) | Windows you interact with (dialogs, preferences, Finder) |

**When to use each:**

- **ignore**: Windows you don't want yashiki to touch at all
  - Examples: Firefox dropdowns, tooltips, autocomplete popups
  - Characteristics: Appear/disappear frequently, not directly interacted with

- **float**: Windows excluded from tiling but still managed by yashiki
  - Examples: Finder, System Settings, dialogs, Quick Terminal
  - Characteristics: User interacts with them, may want to move between tags, should be focusable

```sh
# Example: Completely ignore Firefox popups
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore

# Example: Finder floats but is still managed
yashiki rule-add --app-name Finder float
```

## Rule Specificity

Rules are sorted by specificity - more specific rules take priority. Specificity is calculated as:

1. **Exact match** - highest priority (e.g., `Safari`)
2. **Prefix/suffix match** - medium priority (e.g., `Safari*`, `*Safari`)
3. **Contains match** - lower priority (e.g., `*Safari*`)
4. **Wildcard only** - lowest priority (e.g., `*`)

When multiple rules match a window, each action type uses the first matching rule:

```sh
# More specific rule takes priority
yashiki rule-add --app-name Safari --title "*Preferences*" float
yashiki rule-add --app-name Safari tags 2

# Result: Safari Preferences window floats AND goes to tag 2
# Result: Other Safari windows go to tag 2 (not floating)
```

## Managing Rules

```sh
# List all rules
yashiki list-rules

# Remove a rule (must match exactly)
yashiki rule-del --app-name Finder float

# Rules are evaluated in specificity order, not insertion order
```

## Finding AX Attributes

The `--ax-id` and `--subrole` options use macOS Accessibility API attributes. Here's how to find them.

### Using Accessibility Inspector

Accessibility Inspector is included with Xcode:

1. Open Xcode
2. Menu: Xcode → Open Developer Tool → Accessibility Inspector
3. Click the target button (crosshair icon) in the toolbar
4. Hover over the target window
5. Look for these attributes in the inspector:
   - **AXIdentifier** - Use with `--ax-id`
   - **AXSubrole** - Use with `--subrole`

### Using Terminal

You can query AX attributes using a Swift script. Save this as `ax-inspect.swift`:

```swift
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
```

Run with:

```sh
chmod +x ax-inspect.swift
./ax-inspect.swift
```

This prints attributes of the currently focused window.

### Debugging Window Discovery

To see what windows yashiki discovers and their attributes, run with debug logging:

```sh
RUST_LOG=yashiki=debug yashiki start
```

This will log each discovered window with all its attributes:
```
Discovered window: [12345] pid=1234 app='Firefox' app_id=Some("org.mozilla.firefox") title='Menu' ax_id=None subrole=Some("AXUnknown") layer=0 close=ButtonInfo{exists:true,enabled:Some(true)} fullscreen=ButtonInfo{exists:true,enabled:Some(true)} minimize=ButtonInfo{exists:true,enabled:Some(true)} zoom=ButtonInfo{exists:true,enabled:Some(true)}
```

The log includes:
- `layer` - Window level (0=normal, 3=floating, etc.)
- `close`, `fullscreen`, `minimize`, `zoom` - Button states with exists/enabled info

Use this information to create appropriate rules.

### Subrole Reference

The `--subrole` option accepts values with or without the "AX" prefix:

```sh
# These are equivalent
yashiki rule-add --subrole Dialog float
yashiki rule-add --subrole AXDialog float
```

Common subroles:
- `AXStandardWindow` - Normal windows
- `AXDialog` - Dialog windows
- `AXFloatingWindow` - Floating panels
- `AXSystemDialog` - System dialogs
- `AXUnknown` - Unspecified (often popups, dropdowns, tooltips)

## Common Use Cases

### Floating Dialog Windows

**Problem:** System dialogs and preference windows get tiled with other windows.

**Solution:** Use `--subrole Dialog` to float all dialog windows:

```sh
yashiki rule-add --subrole Dialog float
yashiki rule-add --subrole FloatingWindow float
```

For specific apps:

```sh
yashiki rule-add --app-name "System Preferences" float
yashiki rule-add --app-name "System Settings" float
yashiki rule-add --title "*Preferences*" float
```

### Quick/Popup Windows

**Problem:** Some apps have special popup windows (Quick Terminal, Quick Note, Spotlight-like search) that should float, or temporary popup windows (dropdowns, tooltips) that should be ignored entirely.

**Solution:** Use `--ax-id` with the window's AXIdentifier, or `--subrole` for popup windows:

```sh
# Ghostty Quick Terminal - float (still managed, but excluded from tiling)
yashiki rule-add --ax-id "com.mitchellh.ghostty.quickTerminal" float

# Ignore all AXUnknown windows (Firefox dropdowns, tooltips, etc.)
yashiki rule-add --subrole AXUnknown ignore

# Ignore only Firefox popup windows
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore

# Find the AXIdentifier using Accessibility Inspector or the script above
```

**Note:** The `ignore` action completely skips window management - the window won't appear in `list-windows` or be affected by any yashiki operations. Use `float` if you want the window to be managed but excluded from tiling.

### Assigning Windows to Tags

**Problem:** You want certain apps to always open on a specific tag.

**Solution:**

```sh
# Safari on tag 2 (bitmask 2 = 1<<1)
yashiki rule-add --app-name Safari tags 2

# Slack on tag 3 (bitmask 4 = 1<<2)
yashiki rule-add --app-name Slack tags 4
```

### Moving Windows to Specific Displays

**Problem:** You want certain apps on a specific monitor.

**Solution:**

```sh
# Chrome on display 2
yashiki rule-add --app-name "Google Chrome" output 2

# All Google apps on display 2
yashiki rule-add --app-id "com.google.*" output 2
```

## Troubleshooting

### Rule Not Matching

1. **Check with `list-rules`** - Verify the rule exists:
   ```sh
   yashiki list-rules
   ```

2. **Verify the matcher values** - Use Accessibility Inspector or the Swift script to confirm:
   - App name matches `--app-name`
   - Bundle ID matches `--app-id`
   - Window title matches `--title`
   - AXIdentifier matches `--ax-id`
   - AXSubrole matches `--subrole`

3. **Check glob patterns** - Remember that `*` matches any characters:
   - `Safari` - exact match only
   - `*Safari*` - contains "Safari"
   - `Safari*` - starts with "Safari"

### Window Not Floating

1. **Check if a `no-float` rule exists** - A more specific `no-float` rule may override your `float` rule.

2. **Verify the window is being created, not moved** - Rules only apply to newly created windows. Use `window-toggle-float` for existing windows.

### AXIdentifier Not Found

Not all windows have an AXIdentifier. If Accessibility Inspector shows "(null)" or empty:

1. Try using `--subrole` instead
2. Use `--app-id` combined with `--title` for more specific matching
3. Some windows may not be distinguishable by AX attributes
