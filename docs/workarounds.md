# App Workarounds

This page documents known issues with specific applications and their workarounds using window rules.

## Table of Contents

- [JankyBorders](#jankyborders)
- [Contexts](#contexts)
- [Bartender](#bartender)
- [Firefox](#firefox)
- [Ghostty](#ghostty)
- [Generic Popup/Palette Windows](#generic-popuppalette-windows)
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

## Debugging Window Issues

If you encounter similar issues with other applications, use debug logging to identify problematic windows:

```sh
RUST_LOG=yashiki=debug yashiki start
```

Look for lines like:
```
Discovered window: [12345] pid=1234 app='AppName' app_id=Some("com.example.app") title='' ax_id=None subrole=Some("AXUnknown") layer=0 close=ButtonInfo{exists:true,enabled:Some(true)} fullscreen=ButtonInfo{...} minimize=ButtonInfo{...} zoom=ButtonInfo{...}
```

Then create appropriate rules using:
- `--app-id`, `--app-name`, `--title` - Basic matching
- `--ax-id`, `--subrole` - AX attribute matching
- `--window-level` - Window level matching (normal, floating, other, etc.)
- `--close-button`, `--fullscreen-button`, `--minimize-button`, `--zoom-button` - Button state matching (exists, none, enabled, disabled)
