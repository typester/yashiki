# Yashiki

macOS tiling window manager written in Rust.

## Workspace Notes

- Version control: `jj` (not `git`)
- Before starting work:
  1. Run `jj workspace root` to get the workspace root path
  2. Run `jj status` to confirm current workspace state
- All file edits must target files under the workspace root path
- Update only this CLAUDE.md, not the root one
- **jj write operations (commit, describe, new, etc.) are done by the user, not Claude**
  - For PRs: output title and description text, let the user handle jj/git operations

## Project Structure

```
yashiki/                  # WM core daemon + CLI
yashiki-ipc/              # Shared protocol definitions (commands, layout)
yashiki-layout-tatami/    # Tile layout engine (master-stack)
yashiki-layout-byobu/     # Accordion layout engine (stacked windows)
```

Future: `engawa/` (status bar), `yashiki-layout-rasen` (spiral), `yashiki-layout-koushi` (grid)

## Architecture

### Thread Model

- **Main thread**: CFRunLoop - Accessibility API, global hotkeys (CGEventTap), window operations
- **Tokio runtime**: IPC server (Unix Domain Socket), event forwarding

### Communication

- IPC/Hotkey commands → main thread via `std::sync::mpsc` + `CFRunLoopSource`
- `CFRunLoopSourceSignal` + `CFRunLoopWakeUp` for immediate processing (no polling delay)
- Layout engine: stdin/stdout JSON (synchronous, from main thread)

### Virtual Workspaces (No SIP Required)

Like AeroSpace, uses virtual workspaces instead of macOS native Spaces:
- All windows on single macOS Space, workspace switching hides windows (per-display corner positioning)
- Only uses public Accessibility API, NSScreen visibleFrame for layout area

**Why per-display hiding?** Native fullscreen moves a display to a separate macOS Space where Accessibility API cannot access windows. If windows were hidden to a global position (e.g., bottom-right of all displays), they would become inaccessible when any display enters fullscreen. Per-display hiding ensures each display's windows stay within its own bounds.

## Key Features

- **Multi-monitor support** (river-style) - each display has independent visible tags
- **Tag-based workspace management** (like dwm/river) - windows can have multiple tags (bitmask)
- **External layout engine** (like river) - separate process, stdin/stdout JSON, custom engines supported
- **Per-tag layout switching** - each tag can have different layout engine
- **River-style configuration** - shell script (`~/.config/yashiki/init`), CLI commands
- **Window rules** (riverctl-style) - glob patterns, actions: ignore, float, tags, output, position, dimensions
- **Cursor warp** - `disabled`, `on-output-change`, `on-focus-change`
- **State streaming** - real-time events via `/tmp/yashiki-events.sock`

## Layout Protocol

```rust
// yashiki → layout engine
enum LayoutMessage {
    Layout { width: u32, height: u32, windows: Vec<u32> },
    Command { cmd: String, args: Vec<String> },
}

// layout engine → yashiki
enum LayoutResult {
    Layout { windows: Vec<WindowGeometry> },  // id, x, y, width, height
    Ok,
    NeedsRetile,
    Error { message: String },
}
```

Focus notification: `focus-changed <window_id>` sent automatically on focus change.

## State Streaming

Events via `/tmp/yashiki-events.sock` (JSON lines). Client sends `SubscribeRequest` with optional snapshot and filter. Events: WindowCreated/Destroyed/Updated, WindowFocused, DisplayFocused/Added/Removed/Updated, TagsChanged, LayoutChanged, Snapshot.

## CLI Usage

Tags use bitmask: tag 1 = 1, tag 2 = 2, tag 3 = 4, tags 1+2 = 3

```sh
yashiki start                     # Start daemon
yashiki bind alt-1 tag-view 1     # Bind hotkey
yashiki unbind alt-1              # Unbind hotkey
yashiki list-bindings             # List bindings
yashiki tag-view 1                # Switch to tag
yashiki tag-view --output 2 1     # Switch on specific display
yashiki tag-toggle 2              # Toggle tag visibility
yashiki tag-view-last             # Switch to previous tags
yashiki window-move-to-tag 1      # Move window to tag
yashiki window-toggle-tag 2       # Toggle tag on window
yashiki window-focus next|prev|left|right|up|down
yashiki window-swap next|prev|left|right|up|down
yashiki window-toggle-fullscreen
yashiki window-toggle-float
yashiki window-close
yashiki output-focus next|prev
yashiki output-send next|prev
yashiki retile [--output N]
yashiki layout-set-default tatami
yashiki layout-set [--tags N] [--output N] byobu
yashiki layout-get [--tags N] [--output N]
yashiki layout-cmd [--layout name] <cmd> [args]
yashiki list-windows [--all] [--debug]
yashiki list-outputs
yashiki get-state
yashiki exec "command"
yashiki exec --track "borders"  # Track process, terminate on quit
yashiki exec-or-focus --app-name Safari "open -a Safari"
yashiki set-exec-path|add-exec-path|exec-path
yashiki rule-add --app-name|--app-id|--title|--ax-id|--subrole|--window-level|--*-button <pattern> <action>
yashiki rule-del <matcher> <action>
yashiki list-rules
yashiki set-cursor-warp disabled|on-output-change|on-focus-change
yashiki set-outer-gap <all>|<v h>|<t r b l>
yashiki subscribe [--snapshot] [--filter events]
yashiki quit
```

## Config Example

```sh
#!/bin/sh
# ~/.config/yashiki/init
yashiki add-exec-path /opt/homebrew/bin
yashiki set-cursor-warp on-focus-change
yashiki layout-set-default tatami
yashiki layout-set --tags 4 byobu

# Start companion tools (terminated on yashiki quit)
yashiki exec --track "borders active_color=0xffe1e3e4"

# Tag bindings
yashiki bind alt-1 tag-view 1
yashiki bind alt-shift-1 window-move-to-tag 1
yashiki bind alt-tab window-focus next
yashiki bind alt-f window-toggle-fullscreen

# Layout commands
yashiki set-outer-gap 10
yashiki layout-cmd --layout tatami set-inner-gap 10

# Window rules
yashiki rule-add --app-name Finder float
yashiki rule-add --subrole AXUnknown ignore
```

## Implementation Status

### Core Modules
- **macos/** - Platform bindings: accessibility.rs (AXUIElement), display.rs (CGWindowList, NSScreen), observer.rs (AXObserver), workspace.rs (NSWorkspace), hotkey.rs (CGEventTap)
- **core/** - State management: state/mod.rs, window.rs, display.rs, tag.rs, config.rs, rules_engine.rs
- **ipc/** - server.rs, client.rs, event_server.rs
- **app.rs** - Main event loop (CFRunLoop), effect pattern
- **app/** - Command handling: dispatch.rs (unified command dispatcher), sync_helper.rs (sync+retile helper)
- **layout.rs** - LayoutEngine, LayoutEngineManager
- **platform.rs** - WindowSystem/WindowManipulator traits for testability
- **yashiki-ipc/** - Shared types (Command, Response, LayoutMessage, WindowRule, StateEvent, etc.)

### Layout Engines
- **tatami** - Master-stack layout. Commands: zoom, set-main-ratio, inc/dec-main-count, set-inner-gap
- **byobu** - Accordion layout. Commands: set-padding, set-orientation, toggle-orientation

## Development Notes

- Requires Accessibility permission (System Preferences → Privacy & Security → Accessibility)
- Run daemon: `RUST_LOG=info cargo run -p yashiki -- start`
- Run CLI: `cargo run -p yashiki -- list-windows`
- PID file: `/tmp/yashiki.pid`

## Release & Distribution

Homebrew: `brew tap typester/yashiki && brew install --cask yashiki`

Release process: release-plz creates PR → merge triggers release → release.yml builds app bundles → manually update Homebrew cask

Build locally: `./scripts/build-app.sh --release`

## Dependencies

Key crates: argh, core-foundation, core-foundation-sys, core-graphics, objc2, objc2-app-kit, objc2-foundation, tokio, dirs

## Code Style

- All code in English, minimal comments
- When adding dependencies, use latest version
- Prefer Actor model - single thread data operations, avoid Mutex
- DRY principle - extract helpers for 3+ occurrences

### Use Statement Ordering

1. **std crates** - `std::`, `core::`, `alloc::`
2. **external crates** - third-party
3. **internal crates** - `crate::`, `super::`, `yashiki_ipc`

Each group sorted alphabetically, blank lines between groups.

## Workflow

### ⚠️ CRITICAL: Never modify code without explicit approval

**DO NOT edit any source code (*.rs, etc.) until the user explicitly approves.**

Examples of what is NOT approval:
- Discussing a plan or approach
- Answering questions about implementation details
- User saying the approach "sounds good" or "makes sense"

**You MUST explicitly ask "Should I implement this?" and wait for clear confirmation before editing any code.**

**Workflow:**
1. Analyze the problem and present a plan
2. Ask "Should I implement this?" (or similar)
3. Wait for explicit approval from user
4. Only then use Edit/Write tools on source code
5. Run `cargo fmt --all` at the end of each task

**Review requests = report only, NEVER auto-fix:**
- When asked to "review", "check", or "verify" code: report findings, do NOT modify code
- Wait for explicit approval before making any changes
- This applies even when bugs or issues are found during review

**Documentation updates:**
- Update docs when adding/changing features: README.md, CLAUDE.md, docs/*.md

## Design Decisions

### Hotkey Management
- Bindings in `HashMap<Hotkey, Command>`, dirty flag for deferred tap recreation
- CGEventTap callback signals CFRunLoopSource for immediate processing

### Focus
- `next`/`prev`: Stack-based (sorted by window ID)
- `left`/`right`/`up`/`down`: Geometry-based (Manhattan distance)
- Focus involves: `activate_application(pid)` then `AXUIElement.raise()`
- Electron apps: NSWorkspace.frontmostApplication as primary, accessibility API as fallback

### Multi-monitor
- Each Display has own `visible_tags`, `State.focused_display` tracks focus
- `--output` option targets specific display by ID or name (partial match)
- Window's display determined by center point location

### Monitor Connection/Disconnection
- Polls `CGGetActiveDisplayList` in timer_callback (500ms)
- Orphaned windows moved to fallback display, affected displays retiled

### Coordinate Systems
- NSScreen: origin at main screen's bottom-left, y-axis up
- Core Graphics: origin at main screen's top-left, y-axis down
- Conversion: `cg_y = main_screen_height - ns_y - height` (always use main screen's height)

### External Monitor Menu Bar (macOS Bug Workaround)
`NSScreen.visibleFrame` may not report menu bar on some external monitors. Workaround in `macos/display.rs`: detect menu bar via CGWindowList (Window Server, layer 24), apply adjustment when `visibleFrame == frame`.

### Window Management
- Hidden windows: moved to screen's corner (per-display), `saved_frame` stores original position
- Auto tag switch: when external focus (Dock, Cmd+Tab) changes to hidden window, tag switches automatically
- Per-tag layout: `tag-view` switches layout, `tag-toggle` maintains current, `tag-view-last` swaps with previous

### Window Hiding Constraints

1. **1px Visibility Rule**: At least 1x1 pixel of a window must remain within the display bounds. macOS will automatically reposition windows that are completely offscreen.

2. **Per-Display Hiding**: When a display enters native fullscreen mode, it moves to a new macOS Space where windows cannot be accessed via Accessibility API. Each display must hide its windows independently within its own bounds.

3. **Window Position Reference Point**: macOS window position is the top-left corner. The window body extends right and down from this point. To hide at non-bottom-right corners, the position must be offset by window dimensions:
   - bottom-right: `(display_right - 1, display_bottom - 1)` - no offset needed
   - bottom-left: `(display_left - window_width + 1, display_bottom - 1)`
   - top-right: `(display_right - 1, display_top - window_height + 1)`
   - top-left: `(display_left - window_width + 1, display_top - window_height + 1)`

4. **Corner Selection**: Selects a safe corner where the window body won't extend into adjacent displays. Priority: bottom-right → bottom-left → top-right → top-left.

**Why corner selection matters:** macOS window position is the top-left corner, and the window body extends **right and down**. If display A is to the left of display B and we hide A's window to bottom-right corner, the window body extends into display B and becomes visible there. By selecting a corner away from adjacent displays (e.g., bottom-left for A), we ensure hidden windows stay invisible.

**Related code:**
- `core/state/layout.rs`: `compute_hide_position_for_display()` - per-display hide position calculation

### Cursor Warp
Three modes: Disabled (default), OnOutputChange, OnFocusChange. Uses `CGWarpMouseCursorPosition`.

### Window Rules
- Default tag: new windows inherit display's `visible_tags`
- Sorted by specificity (more specific first), "first match wins" per action type
- Matching: app-name, app-id, title, ax-id, subrole, window-level, button states
- For ax-id/subrole: "none" matches absent attribute
- Non-normal layer windows: not managed by default, any non-ignore rule manages them (default to floating)

### Outer Gap
Managed by daemon (not layout engines), applied to all layouts including fullscreen. CSS-style syntax.

### Popup Filtering
Use `ignore` rule with subrole/ax-id matching. Example: `--subrole AXUnknown ignore`

### Orphan Tracking (Sleep/Wake Window Restoration)

> **⚠️ IMPORTANT FOR FUTURE CHANGES:**
> This design involves intentional trade-offs. Before modifying `orphaned_from` behavior or adding new places that set/clear it, **consult the user** and explain the impact on these documented behaviors.

When displays disconnect (e.g., during sleep), windows are "orphaned" to a fallback display.
The `orphaned_from` field in `Window` tracks the original display for restoration when it returns.

**State transitions:**
- `None` → `Some(display_id)`: Display disconnected, window moved to fallback
- `Some(display_id)` → `None`: Original display returned and window restored, OR user explicitly moved window via Yashiki command
- `Some(display_id)` → `Some(display_id)` (unchanged): Multi-stage disconnect preserves first source

**When `orphaned_from` is cleared:**
- `send_to_output` command (user explicitly moves window between displays)
- Successful restoration when original display returns

**When `orphaned_from` is NOT cleared (intentional):**
- User drags window to another display (indistinguishable from OS-initiated moves)
- Window rules move window to specific display
- OS resolution changes or window repositioning

**Rationale:** Clearing only on explicit Yashiki commands ensures OS callbacks don't accidentally discard orphan state. Trade-off: user drags are not recognized as user intent.

**Known edge cases (accepted trade-offs):**
1. **Rule + orphan conflict**: If user adds a rule moving an orphaned window, restoration may override the rule when original display returns
2. **Drag + restoration**: User-dragged windows may be moved back when original display returns

**Future improvement considerations:**
- Detect user drags via AXUIElement move observation (would require distinguishing user drags from programmatic moves)
- Use display UUID instead of CGDirectDisplayID for more robust identification

**Related code:**
- `core/window.rs`: `orphaned_from` field definition
- `core/state/display.rs`: `handle_display_change()` - orphan/restore logic, `send_to_output()` - clear on user move

## Testing

Run: `cargo test --all`

Tested modules: core/tag.rs, core/state.rs, core/rules_engine.rs, macos/hotkey.rs, yashiki-ipc, app.rs, app/dispatch.rs, app/sync_helper.rs, event_emitter.rs, yashiki-layout-byobu

### Architecture for Testability
- `platform.rs`: WindowSystem trait (queries), WindowManipulator trait (side effects)
- Effect pattern: `process_command()` (pure) returns Effects, `execute_effects()` executes them
- MockWindowSystem/MockWindowManipulator for tests
