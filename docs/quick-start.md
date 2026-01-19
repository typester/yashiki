# Quick Start Guide

Welcome to Yashiki! This guide will help you get up and running with your new tiling window manager.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Granting Accessibility Permission](#granting-accessibility-permission)
- [Understanding Core Concepts](#understanding-core-concepts)
- [Creating Your First Config](#creating-your-first-config)
- [Starting Yashiki](#starting-yashiki)
- [Your First Workflow](#your-first-workflow)
- [Config Examples](#config-examples)
- [Common Issues](#common-issues)
- [Next Steps](#next-steps)

## Prerequisites

- **macOS 12.0 (Monterey) or later**
- **Homebrew** (recommended for installation)

## Installation

### Using Homebrew (Recommended)

```sh
brew tap typester/yashiki
brew install --cask yashiki
```

**Important:** Yashiki.app is not notarized (ad-hoc signed). macOS will show a warning on first launch. You have two options:

1. **After installation:** Go to System Settings → Privacy & Security, scroll down, and click "Open Anyway" when prompted.

2. **During installation:** Install with the `--no-quarantine` flag to skip the warning:
   ```sh
   brew install --cask --no-quarantine yashiki
   ```

The cask installs:
- `Yashiki.app` to `/Applications`
- CLI tools: `yashiki`, `yashiki-layout-tatami`, `yashiki-layout-byobu`

### Using Cargo

If you have Rust installed, you can install from [crates.io](https://crates.io/crates/yashiki):

```sh
# Core daemon and CLI
cargo install yashiki

# Install the layout engines you want to use
cargo install yashiki-layout-tatami   # Master-stack layout
cargo install yashiki-layout-byobu    # Accordion layout
```

## Granting Accessibility Permission

Yashiki needs Accessibility permission to manage windows. This is a one-time setup.

### Step 1: Open System Settings

Go to **System Settings → Privacy & Security → Accessibility**

### Step 2: Add Yashiki

If you installed via Homebrew:
1. Click the **+** button
2. Navigate to `/Applications/` and select **Yashiki.app**
3. Toggle the switch to enable it

If you're running `yashiki start` from terminal (development only):
- Add your terminal app (e.g., Terminal, iTerm2, Ghostty) instead
- This is not recommended for regular use

### Step 3: Verify

After adding, you should see Yashiki.app (or your terminal) listed with the toggle enabled.

## Understanding Core Concepts

Before diving into configuration, let's understand the key concepts that make Yashiki different from other window managers.

### Tags vs Traditional Workspaces

Most window managers use numbered workspaces where each window belongs to exactly one workspace. Yashiki uses **tags** (inspired by dwm/awesomewm/river):

| Traditional Workspaces | Yashiki Tags |
|------------------------|--------------|
| Window belongs to one workspace | Window can have multiple tags |
| View one workspace at a time | View any combination of tags |
| Switch workspaces | Toggle tags on/off |

**The Bitmask System**

Tags are represented as bits in a number (bitmask):

| Tag | Bitmask | Binary | Shell Expression |
|-----|---------|--------|------------------|
| Tag 1 | `1` | `001` | `$((1<<0))` |
| Tag 2 | `2` | `010` | `$((1<<1))` |
| Tag 3 | `4` | `100` | `$((1<<2))` |
| Tag 4 | `8` | `1000` | `$((1<<3))` |
| Tags 1+2 | `3` | `011` | View both tags simultaneously |
| Tags 1+3 | `5` | `101` | View tags 1 and 3 together |

This allows powerful operations like viewing multiple tags at once or assigning a window to multiple tags.

### Virtual Workspaces

Unlike native macOS Spaces, Yashiki uses **virtual workspaces**:

- All windows exist on a single macOS Space
- "Hiding" a window moves it off-screen (to the bottom-right corner)
- No SIP disable required
- Switching tags is instant (no sliding animation)

This approach has tradeoffs:
- **Pros:** Fast, no SIP disable, works with all apps
- **Cons:** Mission Control shows all windows, Cmd+Tab shows all apps

### Layout Engines

Yashiki uses external layout engines (like river) instead of built-in layouts:

- **tatami**: Master-stack layout (one main window + stack)
- **byobu**: Accordion layout (stacked windows with stagger)

Layout engines are separate processes that communicate via JSON, so you can even write your own in any language.

**Tatami Layout (Master-Stack)**

```
+------------------+--------+
|                  |   2    |
|                  +--------+
|        1         |   3    |
|      (main)      +--------+
|                  |   4    |
+------------------+--------+
```

**Byobu Layout (Accordion)**

```
+--+--+--+------------------+
|  |  |  |                  |
|1 |2 |3 |        4         |
|  |  |  |    (focused)     |
|  |  |  |                  |
+--+--+--+------------------+
```

### Executable Configuration

Unlike traditional config files, Yashiki's configuration is an **executable file** that runs when the daemon starts:

- Config file: `~/.config/yashiki/init`
- Any executable works: shell script, Python, Ruby, compiled binary, etc.
- The executable runs CLI commands like `yashiki bind ...` to configure the daemon
- Changes take effect after restarting the daemon

Most users write shell scripts, but you can use any language you prefer:

```sh
#!/usr/bin/env python3
import subprocess
for i in range(1, 10):
    subprocess.run(["yashiki", "bind", f"alt-{i}", "tag-view", str(1 << (i-1))])
```

## Creating Your First Config

Let's create a minimal but functional configuration.

### Step 1: Create the Config Directory

```sh
mkdir -p ~/.config/yashiki
```

### Step 2: Create the Init Script

Create `~/.config/yashiki/init`:

```sh
#!/bin/sh

# Layout configuration
yashiki layout-set-default tatami
yashiki set-outer-gap 10
yashiki layout-cmd --layout tatami set-inner-gap 8

# Tag bindings: alt-1 through alt-9 switch to tags 1-9
# alt-shift-1 through alt-shift-9 move windows to tags 1-9
for i in 1 2 3 4 5 6 7 8 9; do
  yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
  yashiki bind "alt-shift-$i" window-move-to-tag "$((1<<(i-1)))"
done

# Window focus (vim-style)
yashiki bind alt-j window-focus next
yashiki bind alt-k window-focus prev

# Layout adjustment
yashiki bind alt-h layout-cmd dec-main-ratio
yashiki bind alt-l layout-cmd inc-main-ratio

# Window management
yashiki bind alt-f window-toggle-fullscreen
yashiki bind alt-shift-f window-toggle-float
yashiki bind alt-shift-c window-close

# Multi-monitor
yashiki bind alt-o output-focus next
yashiki bind alt-shift-o output-send next
```

### Step 3: Make It Executable

```sh
chmod +x ~/.config/yashiki/init
```

### Understanding the Config

Let's break down what each section does:

**Layout Setup**
```sh
yashiki layout-set-default tatami         # Use tatami as the default layout
yashiki set-outer-gap 10                  # 10px gap between windows and screen edges
yashiki layout-cmd --layout tatami set-inner-gap 8  # 8px gap between windows
```

**Tag Bindings (the loop)**
```sh
for i in 1 2 3 4 5 6 7 8 9; do
  yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
  yashiki bind "alt-shift-$i" window-move-to-tag "$((1<<(i-1)))"
done
```

This loop creates 18 bindings:
- `alt-1` switches to tag 1, `alt-2` to tag 2, etc.
- `alt-shift-1` moves the focused window to tag 1, etc.

The expression `$((1<<(i-1)))` calculates the bitmask: when `i=1`, it's `1<<0=1`; when `i=2`, it's `1<<1=2`; when `i=3`, it's `1<<2=4`, and so on.

## Starting Yashiki

### Launch the App

If you installed via Homebrew, simply open **Yashiki.app** from `/Applications` or Spotlight.

The app will:
1. Start the daemon
2. Execute your init script
3. Begin managing windows

### Verify It's Running

Open a terminal and run:

```sh
yashiki list-windows
```

You should see a list of your open windows with their IDs, titles, and tags.

### Check Your Bindings

```sh
yashiki list-bindings
```

This shows all hotkeys you've configured.

## Your First Workflow

Now let's try out the basic operations. Open a few windows (e.g., Terminal, Safari, Finder) to practice.

### 1. Switch Focus Between Windows

With multiple windows on the same tag:

- Press `alt-j` to focus the next window
- Press `alt-k` to focus the previous window

The windows should tile automatically in the tatami (master-stack) layout.

### 2. Adjust the Layout

- Press `alt-h` to decrease the main area ratio (make it smaller)
- Press `alt-l` to increase the main area ratio (make it larger)

### 3. Move Windows Between Tags

1. Focus a window you want to move
2. Press `alt-shift-2` to move it to tag 2
3. Press `alt-2` to switch to tag 2 and see the window

### 4. Toggle Fullscreen

- Press `alt-f` to make the focused window fullscreen
- Press `alt-f` again to return to tiled mode

This is tiling-style fullscreen within the current Space, not macOS native fullscreen (which creates a separate Space).

### 5. Float a Window

- Press `alt-shift-f` to toggle floating mode on the focused window
- Floating windows can be moved and resized freely
- They're excluded from the tiling layout

### 6. Multi-Monitor (If You Have Multiple Displays)

- Press `alt-o` to focus the next monitor
- Press `alt-shift-o` to send the focused window to the next monitor

Each monitor has its own set of visible tags.

## Config Examples

Here are progressively more advanced configurations.

### Minimal Config

The bare minimum to get started:

```sh
#!/bin/sh
yashiki layout-set-default tatami

for i in 1 2 3 4 5 6; do
  yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
done

yashiki bind alt-j window-focus next
yashiki bind alt-k window-focus prev
```

### Standard Config

A balanced configuration for daily use:

```sh
#!/bin/sh

# Layout
yashiki layout-set-default tatami
yashiki set-outer-gap 10
yashiki layout-cmd --layout tatami set-inner-gap 10

# Cursor warp (mouse follows focus)
yashiki set-cursor-warp on-focus-change

# Tags
for i in 1 2 3 4 5 6 7 8 9; do
  yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
  yashiki bind "alt-shift-$i" window-move-to-tag "$((1<<(i-1)))"
done
yashiki bind alt-0 tag-view-last

# Window focus
yashiki bind alt-j window-focus next
yashiki bind alt-k window-focus prev
yashiki bind alt-h layout-cmd dec-main-ratio
yashiki bind alt-l layout-cmd inc-main-ratio
yashiki bind alt-comma layout-cmd inc-main-count
yashiki bind alt-period layout-cmd dec-main-count

# Window management
yashiki bind alt-f window-toggle-fullscreen
yashiki bind alt-shift-f window-toggle-float
yashiki bind alt-shift-c window-close
yashiki bind alt-z layout-cmd zoom

# Multi-monitor
yashiki bind alt-o output-focus next
yashiki bind alt-shift-o output-send next

# Common float rules
yashiki rule-add --app-name Finder float
yashiki rule-add --app-name "System Settings" float
yashiki rule-add --subrole Dialog float
```

### Power User Config

Full-featured configuration with multiple layouts and advanced rules:

```sh
#!/bin/sh

# Extend PATH for custom layout engines
yashiki add-exec-path /opt/homebrew/bin

# Layout configuration
yashiki layout-set-default tatami
yashiki layout-set --tags 4 byobu              # Tag 3 uses byobu
yashiki set-outer-gap 8
yashiki layout-cmd --layout tatami set-inner-gap 8
yashiki layout-cmd --layout byobu set-padding 40

# Cursor warp
yashiki set-cursor-warp on-focus-change

# Tags (all 9 tags)
for i in 1 2 3 4 5 6 7 8 9; do
  yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
  yashiki bind "alt-shift-$i" window-move-to-tag "$((1<<(i-1)))"
  yashiki bind "alt-ctrl-$i" window-toggle-tag "$((1<<(i-1)))"
done
yashiki bind alt-0 tag-view-last
yashiki bind alt-tab tag-view-last

# Window focus (vim + directional)
yashiki bind alt-j window-focus next
yashiki bind alt-k window-focus prev
yashiki bind alt-left window-focus left
yashiki bind alt-right window-focus right
yashiki bind alt-up window-focus up
yashiki bind alt-down window-focus down

# Layout controls
yashiki bind alt-h layout-cmd dec-main-ratio
yashiki bind alt-l layout-cmd inc-main-ratio
yashiki bind alt-comma layout-cmd inc-main-count
yashiki bind alt-period layout-cmd dec-main-count
yashiki bind alt-z layout-cmd zoom
yashiki bind alt-i layout-cmd inc-inner-gap
yashiki bind alt-shift-i layout-cmd dec-inner-gap

# Window management
yashiki bind alt-f window-toggle-fullscreen
yashiki bind alt-shift-f window-toggle-float
yashiki bind alt-shift-c window-close
yashiki bind alt-return retile

# Multi-monitor
yashiki bind alt-o output-focus next
yashiki bind alt-shift-o output-send next

# App launchers
yashiki bind alt-shift-return exec "open -n /Applications/Ghostty.app"
yashiki bind alt-s exec-or-focus --app-name Safari "open -a Safari"
yashiki bind alt-c exec-or-focus --app-name "Google Chrome" "open -a 'Google Chrome'"

# Window rules - Float dialogs and utilities
yashiki rule-add --app-name Finder float
yashiki rule-add --app-name "System Settings" float
yashiki rule-add --app-name "System Preferences" float
yashiki rule-add --subrole Dialog float
yashiki rule-add --subrole FloatingWindow float
yashiki rule-add --fullscreen-button none float

# Window rules - Ignore popups (prevents flickering)
yashiki rule-add --subrole AXUnknown ignore
yashiki rule-add --close-button none ignore

# Window rules - App-specific
yashiki rule-add --app-name Safari tags 2                    # Safari on tag 2
yashiki rule-add --app-name Slack tags 8                     # Slack on tag 4
yashiki rule-add --app-name "Google Chrome" output 2         # Chrome on external monitor
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore  # Firefox popups

# Ghostty Quick Terminal
yashiki rule-add --app-id com.mitchellh.ghostty --fullscreen-button disabled ignore
```

## Common Issues

### Windows Not Tiling

**Symptoms:** Windows are not arranged automatically.

**Solutions:**
1. Verify Yashiki is running: `yashiki list-windows`
2. Check Accessibility permission is granted
3. Try manual retile: `yashiki retile`
4. Some windows may be floating - check with `yashiki list-windows`

### Hotkeys Not Working

**Symptoms:** Pressing configured hotkeys does nothing.

**Solutions:**
1. Verify bindings are registered: `yashiki list-bindings`
2. Make sure your init script is executable: `chmod +x ~/.config/yashiki/init`
3. Restart Yashiki: `yashiki quit` then relaunch the app
4. Check for conflicting system hotkeys in System Settings → Keyboard → Keyboard Shortcuts

### Accessibility Permission Not Working

**Symptoms:** Yashiki can't control windows even after granting permission.

**Solutions:**
1. Remove Yashiki from Accessibility list
2. Restart Yashiki
3. Re-add and re-enable the permission
4. Try rebooting your Mac if issues persist

### Config Not Loading

**Symptoms:** Changes to `~/.config/yashiki/init` have no effect.

**Solutions:**
1. Verify the file exists: `ls -la ~/.config/yashiki/init`
2. Make sure it's executable: `chmod +x ~/.config/yashiki/init`
3. Check for syntax errors: `sh -n ~/.config/yashiki/init`
4. Restart Yashiki to reload the config

### Firefox/Electron App Flickering

**Symptoms:** Windows flicker or layout constantly recalculates.

**Cause:** Some apps create temporary popup windows that trigger relayout.

**Solution:** Add ignore rules for these windows:

```sh
# Firefox popups
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore

# Generic fix for all popups
yashiki rule-add --subrole AXUnknown ignore
yashiki rule-add --close-button none ignore
```

See [workarounds.md](workarounds.md) for more app-specific solutions.

### Windows Appearing on Wrong Display

**Symptoms:** New windows appear on an unexpected monitor.

**Solutions:**
1. Use `output` rules to control window placement:
   ```sh
   yashiki rule-add --app-name "Google Chrome" output 2
   ```
2. Check which display is which: `yashiki list-outputs`

## Next Steps

Now that you have Yashiki up and running, here are some resources to explore further:

### Documentation

- **[Window Rules Guide](window-rules.md)** - Comprehensive guide to window rules including finding AX attributes
- **[Layout Engine Protocol](layout-engine.md)** - Create your own custom layout engine
- **[App Workarounds](workarounds.md)** - Solutions for app-specific issues

### CLI Reference

Run `yashiki` without arguments to see all available commands, or check the [README.md](../README.md) for the full CLI reference.

### Tips

- Use `yashiki get-state` to see the current state of all windows and displays
- Use `RUST_LOG=yashiki=debug yashiki start` to debug issues
- Check `yashiki list-windows` to see which windows are managed and their tags

### Getting Help

- **Issues:** [github.com/typester/yashiki/issues](https://github.com/typester/yashiki/issues)
- **Source Code:** [github.com/typester/yashiki](https://github.com/typester/yashiki)
