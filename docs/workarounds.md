# App Workarounds

This page documents known issues with specific applications and their workarounds using window rules.

## Table of Contents

- [JankyBorders](#jankyborders)
- [Contexts](#contexts)
- [Bartender](#bartender)
- [Firefox](#firefox)
- [Microsoft Outlook](#microsoft-outlook)
- [Ghostty](#ghostty)
- [IINA](#iina)
- [Generic Popup/Palette Windows](#generic-popuppalette-windows)
- [FortiClient](#forticlient)
- [Debugging Window Issues](#debugging-window-issues)

## JankyBorders

JankyBorders creates overlay windows for window border decorations. These windows shouldn't be managed by yashiki.

```sh
yashiki rule-add --app-name borders ignore
```

## Contexts

Contexts is a window switcher that creates overlay windows. These windows shouldn't be managed by yashiki.

```sh
yashiki rule-add --app-name Contexts ignore
```

## Bartender

Bartender is a menu bar manager that creates overlay windows. These windows shouldn't be managed by yashiki.

```sh
yashiki rule-add --app-name "Bartender 4" ignore
```

## Firefox

Firefox creates temporary popup windows (dropdowns, autocomplete, etc.) that can cause flickering during window tiling.

**Recommended rules:**

```sh
# Ignore popup windows with AXUnknown subrole (dropdowns, autocomplete, etc.)
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore

# Float PiP (Picture-in-Picture) and other floating-level windows
yashiki rule-add --app-id org.mozilla.firefox --window-level floating float
```

Add these rules to your `~/.config/yashiki/init` file.

## Microsoft Outlook

Outlook creates various windows that can interfere with window management.

**Recommended rules:**

```sh
# Ignore popup windows with AXUnknown subrole (dropdowns, menus, etc.)
yashiki rule-add --app-id com.microsoft.Outlook --subrole AXUnknown ignore

# Ignore invisible windows with no AX attributes
yashiki rule-add --app-id com.microsoft.Outlook --ax-id none --subrole none ignore
```

The first rule is similar to Firefox - it ignores popup menus and dropdowns. The second rule ignores mysterious invisible windows that Outlook creates without any accessibility attributes.

## Ghostty

### Ghostty Quick Terminal

Ghostty's Quick Terminal has fullscreen button disabled but close button enabled. To ignore it:

```sh
yashiki rule-add --app-id com.mitchellh.ghostty --fullscreen-button disabled --close-button enabled ignore
```

Alternatively, using `--ax-id`:

```sh
yashiki rule-add --ax-id "com.mitchellh.ghostty.quickTerminal" ignore
```

## IINA

IINA is a modern, open-source media player for macOS built on mpv. IINA doesn't allow free window resizing (maintains aspect ratio), so it should float.

```sh
yashiki rule-add --app-id com.colliderli.iina float
```

## Generic Popup/Palette Windows

Many apps create non-normal windows (palettes, panels, tooltips) that shouldn't be tiled. You can ignore all non-normal windows:

```sh
# Ignore all windows with non-normal window level
yashiki rule-add --window-level other ignore
```

Or float/ignore windows without standard buttons:

```sh
# Float windows without fullscreen button (likely dialogs/sheets)
yashiki rule-add --fullscreen-button none float

# Ignore windows without close button (likely popups/tooltips)
yashiki rule-add --close-button none ignore
```

## FortiClient

FortiClient is a VPN client that does not support macOS Accessibility notifications (`kAXErrorNotificationUnsupported`). This means yashiki cannot detect when FortiClient windows are created or closed.

**Symptoms:**
- Window creation is not detected (window doesn't get managed until app is activated via Dock)
- Window close is not detected (ghost window remains in yashiki's state)
- Window cannot be resized (fixed-size window)

Since FortiClient windows cannot be resized and events cannot be tracked, it's best to ignore them entirely:

```sh
yashiki rule-add --app-id com.fortinet.FortiClient ignore
```

**Note:** This is a limitation of FortiClient's implementation, not yashiki. Some enterprise/security applications intentionally disable Accessibility features.
