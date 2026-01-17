# Yashiki

macOS tiling window manager written in Rust.

## Project Structure

```
yashiki/                  # WM core daemon + CLI
yashiki-ipc/              # Shared protocol definitions (commands, layout)
yashiki-layout-tatami/    # Default tile layout engine (master-stack)
```

Future components:
- `engawa/` - Status bar
- Other layout engines: `yashiki-layout-rasen` (spiral), `yashiki-layout-koushi` (grid)

Layout engine naming convention: `yashiki-layout-<name>` (e.g., `yashiki-layout-tatami`)

## Architecture

### Thread Model

- **Main thread**: CFRunLoop
  - Accessibility API (AXUIElement, AXObserver)
  - Global hotkeys (CGEventTap)
  - Window operations
- **Tokio runtime** (separate thread):
  - IPC server (Unix Domain Socket)
  - Event forwarding

### Communication

- IPC commands: tokio → main thread via `std::sync::mpsc`
- Hotkey commands: CGEventTap callback → main thread via `std::sync::mpsc`
- Layout engine: stdin/stdout JSON (synchronous, from main thread)

### Virtual Workspaces (No SIP Required)

Like AeroSpace, uses virtual workspaces instead of macOS native Spaces:
- All windows exist on a single macOS Space
- Workspace switching hides windows AeroSpace-style (position window's top-left at screen's bottom-right corner)
- Only uses public Accessibility API
- Uses NSScreen visibleFrame (excludes menu bar and dock) for accurate layout area

## Key Features

- **Multi-monitor support** (river-style)
  - Each display has independent visible tags
  - Tag operations affect the focused display
  - Windows belong to a display (determined by center point)
  - Layout applied per-display
- **Tag-based workspace management** (like dwm/awesomewm/river)
  - Windows can have multiple tags (bitmask)
  - View any combination of tags
- **External layout engine** (like river)
  - Layout engine is a separate process
  - Communicates via stdin/stdout JSON
  - Layout engine manages its own state (main_count, main_ratio)
  - Users can write custom layout engines
- **River-style configuration**
  - Config is a shell script (`~/.config/yashiki/init`)
  - Uses CLI commands for configuration
  - Dynamic binding changes supported

## Layout Protocol

```rust
// yashiki → layout engine (yashiki-ipc/src/layout.rs)
enum LayoutMessage {
    Layout { width: u32, height: u32, windows: Vec<u32> },  // window IDs
    Command { cmd: String, args: Vec<String> },
}

// layout engine → yashiki
enum LayoutResult {
    Layout { windows: Vec<WindowGeometry> },  // id, x, y, width, height
    Ok,
    Error { message: String },
}
```

## CLI Usage

```sh
yashiki                           # Show help
yashiki start                     # Start daemon
yashiki version                   # Show version
yashiki bind alt-1 view-tag 1     # Bind hotkey
yashiki unbind alt-1              # Unbind hotkey
yashiki list-bindings             # List all bindings
yashiki view-tag 1                # Switch to tag 1
yashiki toggle-view-tag 2         # Toggle tag 2 visibility
yashiki view-tag-last             # Switch to previous tag
yashiki move-to-tag 1             # Move focused window to tag 1
yashiki focus-window next         # Focus next window
yashiki focus-window prev         # Focus previous window
yashiki focus-window left         # Focus window to the left
yashiki focus-output next         # Focus next display
yashiki focus-output prev         # Focus previous display
yashiki send-to-output next       # Move focused window to next display
yashiki send-to-output prev       # Move focused window to previous display
yashiki retile                    # Apply layout
yashiki layout-cmd set-main-ratio 0.6   # Send command to layout engine
yashiki layout-cmd inc-main-count       # Increase main window count
yashiki list-windows              # List all windows
yashiki get-state                 # Get current state
yashiki exec "open -a Safari"     # Execute shell command
yashiki exec-or-focus --app-name Safari "open -a Safari"  # Focus if running, else exec
yashiki quit                      # Quit daemon
```

## Config Example

```sh
# ~/.config/yashiki/init
#!/bin/sh
yashiki bind alt-1 view-tag 1
yashiki bind alt-2 view-tag 2
yashiki bind alt-shift-1 move-to-tag 1
yashiki bind alt-shift-2 move-to-tag 2
yashiki bind alt-tab focus-window next
yashiki bind alt-shift-tab focus-window prev
yashiki bind alt-return retile
yashiki bind alt-comma layout-cmd inc-main-count
yashiki bind alt-period layout-cmd dec-main-count
yashiki bind alt-h layout-cmd dec-main-ratio
yashiki bind alt-l layout-cmd inc-main-ratio
yashiki bind alt-o focus-output next
yashiki bind alt-shift-o send-to-output next

# Gap configuration
yashiki layout-cmd set-inner-gap 10
yashiki layout-cmd set-outer-gap 10
yashiki layout-cmd set-smart-gaps off

# App launchers
yashiki bind alt-return exec "open -n /Applications/Ghostty.app"
yashiki bind alt-s exec-or-focus --app-name Safari "open -a Safari"
```

## Implementation Status

### Completed
- **macos/accessibility.rs** - AXUIElement FFI bindings
  - Permission check, window manipulation (position, size), `raise()` for focus
- **macos/display.rs** - CGWindowList window enumeration, display info
  - `get_on_screen_windows()`, `get_all_displays()` (uses NSScreen visibleFrame)
- **macos/observer.rs** - AXObserver for window events
- **macos/workspace.rs** - NSWorkspace app launch/terminate notifications, `activate_application()`, `get_frontmost_app_pid()`, `exec_command()`
- **macos/hotkey.rs** - CGEventTap global hotkeys
  - `HotkeyManager` with dynamic bind/unbind
  - Tap recreation on binding changes
- **core/display.rs** - Display struct with visible_tags per display
- **core/state.rs** - Window and display state management
  - Multi-monitor: `displays`, `focused_display`, per-display visible_tags
  - Tag operations: `view_tag()`, `toggle_view_tag()`, `move_focused_to_tag()`, `toggle_focused_window_tag()`
  - Focus: `focus_window()` - stack-based (next/prev) and geometry-based (left/right/up/down)
  - Output: `focus_output()`, `send_to_output()` - move focus/window between displays
- **core/window.rs** - Window struct with tags, display_id, saved_frame for off-screen
- **core/tag.rs** - Tag bitmask
- **ipc/server.rs** - IPC server on `/tmp/yashiki.sock`
- **ipc/client.rs** - IPC client for CLI
- **layout.rs** - `LayoutEngine` for spawning and communicating with tatami
- **app.rs** - Main event loop with CFRunLoop timer
  - Processes: IPC commands, hotkey commands, workspace events, observer events
  - Auto-retile on window add/remove
  - Runs init script at startup
- **main.rs** - Daemon + CLI mode
- **yashiki-ipc/** - Command/Response/LayoutMessage enums

### yashiki-layout-tatami (layout engine)
- Master-stack layout
- Internal state: main_count, main_ratio, inner_gap, outer_gap, smart_gaps
- Commands:
  - `set-main-ratio <0.1-0.9>`, `inc-main-ratio [delta]`, `dec-main-ratio [delta]` (default delta: 0.05)
  - `inc-main-count`, `dec-main-count`, `set-main-count <n>`
  - `set-inner-gap <px>`, `set-outer-gap <px>` - gap between windows / screen edges
  - `inc-inner-gap [delta]`, `dec-inner-gap [delta]`, `inc-outer-gap [delta]`, `dec-outer-gap [delta]`
  - `set-smart-gaps <on|off>` - disable gaps when only one window (default: on)

### Not Yet Implemented
- `SwapWindow` command (swap positions with window in direction)
- `CloseWindow` / `ToggleFloat`

## Development Notes

- Requires Accessibility permission (System Preferences → Privacy & Security → Accessibility)
- During development, grant permission to the terminal (e.g., Ghostty)
- Run daemon: `RUST_LOG=info cargo run -p yashiki -- start`
- Run CLI: `cargo run -p yashiki -- list-windows`
- PID file: `/tmp/yashiki.pid` (prevents double startup)

## Dependencies

Key crates:
- `argh` - CLI argument parsing
- `core-foundation` (0.10) - macOS Core Foundation bindings
- `core-graphics` (0.25) - CGWindowList, CGEventTap, display info
- `objc2`, `objc2-app-kit`, `objc2-foundation` - NSScreen, NSWorkspace bindings
- `tokio` - async runtime for IPC server
- `dirs` - config directory location

## Code Style

- All code in English
- Minimal comments - only where logic is non-obvious
- No unnecessary comments explaining what the next line does
- When adding dependencies, always use the latest version
- Prefer Actor model - keep data operations within single thread, avoid Mutex

## Workflow

- When user asks to plan something, present the plan first and wait for approval before implementing
- Do not start implementation until user confirms the plan
- Run `cargo fmt --all` at the end of each task

## Design Decisions

### Hotkey Dynamic Update
- Bindings stored in `HashMap<Hotkey, Command>` on main thread
- When `bind`/`unbind` called, CGEventTap is recreated with new bindings clone
- No Mutex needed - tap callback gets owned clone of bindings

### Focus Direction
Implemented in core (layout-agnostic):
- `next`/`prev`: Stack-based, cycles through windows sorted by window ID
- `left`/`right`/`up`/`down`: Geometry-based, finds nearest window using Manhattan distance

Focus involves: `activate_application(pid)` then `AXUIElement.raise()`

### Focus Tracking (Robust for Electron Apps)
- `get_focused_window()` uses NSWorkspace.frontmostApplication as primary method
- Falls back to accessibility API if NSWorkspace fails
- Electron apps (e.g., Microsoft Teams) often fail with accessibility API (-25212 kAXErrorNoValue)
- `sync_focused_window_with_hint(pid)` provides PID-based fallback for ApplicationActivated events

### Multi-monitor (river-style)
- Each `Display` has its own `visible_tags`
- `State.focused_display` tracks which display has focus
- Focus changes update `focused_display` based on window's `display_id`
- Tag operations (`view_tag`, etc.) affect only `focused_display`
- Window's display determined by center point location
- Layout applied independently per display with display offset
- `focus_output`: cycles displays by sorted ID, focuses first visible window on target
- `send_to_output`: moves window to target display, updates `focused_display`, retiles both displays

### Window Hiding (AeroSpace-style)
- Hidden windows are moved to screen's bottom-right corner (top-left of window at bottom-right of screen)
- Window size is preserved (no resize to 1x1)
- `Window.saved_frame` stores original position when hidden
- `Window.is_hidden()` returns true when `saved_frame.is_some()`
- macOS clamps window positions, so left-edge hiding (-10000) doesn't work reliably

### Automatic Tag Switching on External Focus
- When focus changes externally (Dock, Cmd+Tab, emacsclient, etc.), tag switches automatically
- If focused window is hidden (on different tag), yashiki switches to that window's tag
- Unlike Wayland compositors, macOS cannot prevent external focus changes
- This ensures the focused window is always visible
