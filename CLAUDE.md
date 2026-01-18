# Yashiki

macOS tiling window manager written in Rust.

## Project Structure

```
yashiki/                  # WM core daemon + CLI
yashiki-ipc/              # Shared protocol definitions (commands, layout)
yashiki-layout-tatami/    # Tile layout engine (master-stack)
yashiki-layout-byobu/     # Accordion layout engine (stacked windows)
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

- IPC commands: tokio → main thread via `std::sync::mpsc` + `CFRunLoopSource`
  - Commands are queued via mpsc channel
  - `CFRunLoopSourceSignal` + `CFRunLoopWakeUp` wakes up main thread immediately
  - No polling delay for IPC command processing
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
- **Per-tag layout switching**
  - Each tag can have a different layout engine (tatami, byobu, etc.)
  - Layout engines are spawned lazily (on first use)
  - `tag-toggle` maintains current layout, `tag-view` switches to tag's layout
  - `tag-view-last` restores previous layout along with previous tags
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
    NeedsRetile,  // command succeeded, requests retile
    Error { message: String },
}
```

### Focus Notification

yashiki automatically sends `focus-changed <window_id>` to the layout engine when focus changes.
This allows layout engines to track the focused window without explicit user commands.

Layout engines can return `NeedsRetile` to request a retile after focus changes:
- **tatami**: Returns `Ok` (no retile needed - focus doesn't affect layout)
- **byobu**: Returns `NeedsRetile` (focused window moves to front)

## CLI Usage

Tags use bitmask format: tag 1 = `$((1<<0))` = 1, tag 2 = `$((1<<1))` = 2, tags 1+2 = 3

```sh
yashiki                           # Show help
yashiki start                     # Start daemon
yashiki version                   # Show version
yashiki bind alt-1 tag-view 1     # Bind hotkey (tag 1 = bitmask 1)
yashiki unbind alt-1              # Unbind hotkey
yashiki list-bindings             # List all bindings
yashiki tag-view 1                # Switch to tag 1 (bitmask 1)
yashiki tag-view $((1<<1))        # Switch to tag 2 (bitmask 2)
yashiki tag-view --output 2 1     # Switch to tag 1 on display 2
yashiki tag-toggle 2              # Toggle tag 2 visibility
yashiki tag-toggle --output "DELL" 2  # Toggle on display by name (partial match)
yashiki tag-view-last             # Switch to previous tags
yashiki window-move-to-tag 1      # Move focused window to tag 1
yashiki window-toggle-tag 2       # Toggle tag 2 on focused window
yashiki window-focus next         # Focus next window
yashiki window-focus prev         # Focus previous window
yashiki window-focus left         # Focus window to the left
yashiki window-swap next          # Swap with next window (not yet implemented)
yashiki focused-window            # Get focused window ID
yashiki output-focus next         # Focus next display
yashiki output-focus prev         # Focus previous display
yashiki output-send next          # Move focused window to next display
yashiki output-send prev          # Move focused window to previous display
yashiki retile                    # Apply layout (all displays)
yashiki retile --output 1         # Apply layout on display 1 only
yashiki layout-set-default tatami       # Set default layout engine
yashiki layout-set byobu                # Set layout for current tag
yashiki layout-set --tags 2 byobu       # Set layout for specific tags
yashiki layout-set --output 2 byobu     # Set layout on specific display
yashiki layout-get                      # Get current layout
yashiki layout-get --tags 2             # Get layout for specific tags
yashiki layout-get --output 2           # Get layout for specific display
yashiki layout-cmd set-main-ratio 0.6   # Send command to current layout engine
yashiki layout-cmd --layout tatami set-outer-gap 10  # Send command to specific layout
yashiki layout-cmd inc-main-count       # Increase main window count
yashiki layout-cmd zoom                 # Move focused window to main area (tatami)
yashiki layout-cmd zoom 123             # Move specific window to main area (tatami)
yashiki list-windows              # List all windows
yashiki list-outputs              # List all displays/outputs
yashiki get-state                 # Get current state
yashiki exec "open -a Safari"     # Execute shell command
yashiki exec-or-focus --app-name Safari "open -a Safari"  # Focus if running, else exec
yashiki exec-path                 # Get current exec path
yashiki set-exec-path "/opt/homebrew/bin:$(yashiki exec-path)"  # Set exec path
yashiki quit                      # Quit daemon
```

## Config Example

```sh
# ~/.config/yashiki/init
#!/bin/sh

# Extend exec path (for layout engines and exec commands)
yashiki set-exec-path "/opt/homebrew/bin:$(yashiki exec-path)"

# Layout configuration (per-tag)
yashiki layout-set-default tatami       # Default layout for all tags
yashiki layout-set --tags 4 byobu       # Tag 3 uses byobu layout (bitmask 4 = 1<<2)

# Layout toggle script example (save as ~/.config/yashiki/toggle-layout.sh)
# current=$(yashiki layout-get)
# if [ "$current" = "tatami" ]; then
#   yashiki layout-set byobu
# else
#   yashiki layout-set tatami
# fi
# Usage: yashiki bind alt-space exec ~/.config/yashiki/toggle-layout.sh

# Tag bindings (using bitmask: tag N = $((1<<(N-1))))
yashiki bind alt-1 tag-view 1           # Tag 1 = bitmask 1
yashiki bind alt-2 tag-view 2           # Tag 2 = bitmask 2
yashiki bind alt-3 tag-view 4           # Tag 3 = bitmask 4
yashiki bind alt-shift-1 window-move-to-tag 1
yashiki bind alt-shift-2 window-move-to-tag 2
yashiki bind alt-shift-3 window-move-to-tag 4
yashiki bind alt-tab window-focus next
yashiki bind alt-shift-tab window-focus prev
yashiki bind alt-return retile
yashiki bind alt-comma layout-cmd inc-main-count
yashiki bind alt-period layout-cmd dec-main-count
yashiki bind alt-h layout-cmd dec-main-ratio
yashiki bind alt-l layout-cmd inc-main-ratio
yashiki bind alt-o output-focus next
yashiki bind alt-shift-o output-send next

# Gap configuration (--layout sends to specific engine, without sends to current)
yashiki layout-cmd --layout tatami set-inner-gap 10
yashiki layout-cmd --layout tatami set-outer-gap 10
yashiki layout-cmd --layout byobu set-padding 30

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
- **macos/workspace.rs** - NSWorkspace app launch/terminate notifications, display change notifications, `activate_application()`, `get_frontmost_app_pid()`, `exec_command()`
- **macos/hotkey.rs** - CGEventTap global hotkeys
  - `HotkeyManager` with dynamic bind/unbind
  - Deferred tap recreation via dirty flag (batches multiple bind/unbind calls)
- **core/display.rs** - Display struct with visible_tags per display
- **core/state.rs** - Window and display state management
  - Multi-monitor: `displays`, `focused_display`, per-display visible_tags
  - Tag operations: `view_tags()`, `toggle_tags_on_display()`, `move_focused_to_tags()`, `toggle_focused_window_tags()`
  - Focus: `focus_window()` - stack-based (next/prev) and geometry-based (left/right/up/down)
  - Output: `focus_output()`, `send_to_output()` - move focus/window between displays
  - Display targeting: `resolve_output()`, `get_target_display()` - resolve OutputSpecifier to DisplayId
  - Display change: `handle_display_change()` - handle monitor connect/disconnect
- **core/window.rs** - Window struct with tags, display_id, saved_frame for off-screen
- **core/tag.rs** - Tag bitmask
- **ipc/server.rs** - IPC server on `/tmp/yashiki.sock`
- **ipc/client.rs** - IPC client for CLI
- **layout.rs** - `LayoutEngine` and `LayoutEngineManager`
  - `LayoutEngine` spawns and communicates with a single layout engine process
  - `LayoutEngineManager` manages multiple engines with lazy spawning
- **app.rs** - Main event loop with CFRunLoop
  - CFRunLoopSource for immediate IPC command processing
  - CFRunLoop timer (50ms) for hotkey commands, workspace events, observer events, and deferred hotkey tap updates
  - Auto-retile on window add/remove
  - Runs init script at startup
  - Effect pattern: `process_command()` (pure) + `execute_effects()` (side effects)
- **effect.rs** - Effect enum and CommandResult for separating pure computation from side effects
- **platform.rs** - Platform abstraction layer
  - `WindowSystem` trait for querying window/display info
  - `WindowManipulator` trait for window manipulation side effects
  - `MacOSWindowSystem` / `MacOSWindowManipulator` - Production implementations
  - `MockWindowSystem` / `MockWindowManipulator` - Test implementations
- **main.rs** - Daemon + CLI mode
- **yashiki-ipc/** - Command/Response/LayoutMessage enums, OutputSpecifier, OutputInfo

### yashiki-layout-tatami (layout engine)
- Master-stack layout
- Internal state: main_count, main_ratio, inner_gap, outer_gap, focused_window_id, main_window_id
- Commands:
  - `focus-changed <window_id>` - notification from yashiki (returns `Ok`)
  - `zoom [window_id]` - set main window (uses focused window if id omitted)
  - `set-main-ratio <0.1-0.9>`, `inc-main-ratio [delta]`, `dec-main-ratio [delta]` (default delta: 0.05)
  - `inc-main-count`, `dec-main-count`, `set-main-count <n>`
  - `set-inner-gap <px>` - gap between windows
  - `set-outer-gap <all> | <v h> | <t r b l>` - gap from screen edges (CSS-style: 1/2/4 values)
  - `inc-inner-gap [delta]`, `dec-inner-gap [delta]`, `inc-outer-gap [delta]`, `dec-outer-gap [delta]`

### yashiki-layout-byobu (layout engine)
- Accordion layout (AeroSpace-style)
- Focused window always at rightmost/frontmost position
- Windows staggered incrementally (each offset by `index * padding`)
- All windows have same size, leaving room for all tabs
- Internal state: padding, orientation, focused_window_id
- Commands:
  - `focus-changed <window_id>` - notification from yashiki (returns `NeedsRetile`)
  - `set-padding <px>`, `inc-padding [delta]`, `dec-padding [delta]` (default: 30px, delta: 5px)
  - `set-orientation <horizontal|h|vertical|v>`, `toggle-orientation`

### Not Yet Implemented
- `WindowSwap` command - CLI parsing done, but handler not implemented
- `WindowClose` / `WindowToggleFloat`

## Development Notes

- Requires Accessibility permission (System Preferences → Privacy & Security → Accessibility)
- During development, grant permission to the terminal (e.g., Ghostty)
- Run daemon: `RUST_LOG=info cargo run -p yashiki -- start`
- Run CLI: `cargo run -p yashiki -- list-windows`
- PID file: `/tmp/yashiki.pid` (prevents double startup)

## Release & Distribution

### Homebrew

Users install via Homebrew cask:
```sh
brew tap typester/yashiki
brew install --cask yashiki
```

Homebrew tap repository: `typester/homebrew-yashiki` (separate repo)

### Release Process

1. **release-plz** creates release PR with version bumps and changelog
2. Merging the PR triggers `release-plz release` which creates GitHub releases
3. `release.yml` detects new `yashiki-v*` releases and builds app bundles
4. App bundles (arm64, x86_64) are uploaded to GitHub releases
5. Manually update Homebrew cask formula with new version and SHA256

### App Bundle

Build locally:
```sh
./scripts/build-app.sh --release                    # Current arch
./scripts/build-app.sh --target aarch64-apple-darwin --release  # ARM64
./scripts/build-app.sh --target x86_64-apple-darwin --release   # Intel
```

Output: `target/Yashiki.app` and `target/Yashiki-{arch}-{version}.zip`

**Bundle structure:**
```
Yashiki.app/
  Contents/
    MacOS/
      yashiki              # Main binary
      yashiki-launcher     # Wrapper script (runs `yashiki start`)
    Resources/
      layouts/
        yashiki-layout-tatami
        yashiki-layout-byobu
    Info.plist
```

**Note:** App is ad-hoc signed (not notarized). Users need to allow in System Settings or use `--no-quarantine`.

## Dependencies

Key crates:
- `argh` - CLI argument parsing
- `core-foundation` (0.10) - macOS Core Foundation bindings
- `core-foundation-sys` (0.8) - Low-level Core Foundation FFI (CFRunLoopSource, etc.)
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
- `bind()`/`unbind()` sets `dirty = true` without recreating tap
- `ensure_tap()` called in timer callback (50ms interval) recreates tap only if dirty
- This batches multiple bind/unbind calls during init script execution
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
- Tag operations (`tag-view`, etc.) affect only `focused_display` by default
- `--output` option allows targeting specific display by ID or name (partial match)
- Window's display determined by center point location
- Layout applied independently per display with display offset
- `output-focus`: cycles displays by sorted ID, focuses first visible window on target
- `output-send`: moves window to target display, updates `focused_display`, retiles both displays

### Monitor Disconnection Handling
- Listens for `NSApplicationDidChangeScreenParametersNotification`
- When a display is disconnected:
  - Orphaned windows are moved to fallback display (main display preferred)
  - `focused_display` is updated if it was on the disconnected display
  - Affected displays are automatically retiled
- `handle_display_change()` in State handles the logic

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

### Per-Tag Layout Switching
- `State` holds `default_layout: String` and `tag_layouts: HashMap<u8, String>`
- `Display` holds `current_layout: Option<String>` and `previous_layout: Option<String>`
- Layout determination logic:
  | Operation | Layout Behavior |
  |-----------|-----------------|
  | `tag-view N` | Switch to `tag_layouts[first_tag(N)]` or `default_layout` |
  | `tag-toggle N` | **Maintain** current layout (no change) |
  | `tag-view-last` | Swap `current_layout` ↔ `previous_layout` |
  | `layout-set <layout>` | Set for current tag + immediate retile |
  | `layout-set --tags N <layout>` | Set for tag N (applied when switching to that tag) |
- `LayoutEngineManager` spawns engines lazily on first use and keeps them running
- Each engine maintains its own state (main_ratio, gaps, etc.) independently
- `layout-cmd` sends commands to layout engines
  - Without `--layout`: sends to current active layout and retiles
  - With `--layout <name>`: sends to specified layout (lazy spawns if needed), no retile

## Testing

### Current Test Coverage (75 tests)

Run tests: `cargo test --all`

**Tested modules:**
- `core/tag.rs` - Tag bitmask operations (7 tests)
- `macos/hotkey.rs` - `parse_hotkey()`, `format_hotkey()` (15 tests)
- `yashiki-ipc` - Command/Response/LayoutMessage serialization (21 tests)
- `core/state.rs` - State management with MockWindowSystem (13 tests)
- `app.rs` - `process_command()` effect generation (9 tests)
- `yashiki-layout-byobu` - Accordion layout and commands (9 tests)

### Platform Abstraction Layer

`platform.rs` provides traits for testability:

```rust
// For querying window/display information
pub trait WindowSystem {
    fn get_on_screen_windows(&self) -> Vec<WindowInfo>;
    fn get_all_displays(&self) -> Vec<DisplayInfo>;
    fn get_focused_window(&self) -> Option<FocusedWindowInfo>;
}

// For window manipulation side effects
pub trait WindowManipulator {
    fn apply_window_moves(&self, moves: &[WindowMove]);
    fn apply_layout(&self, display_id: DisplayId, frame: &Rect, geometries: &[WindowGeometry]);
    fn focus_window(&self, window_id: u32, pid: i32);
    fn move_window_to_position(&self, window_id: u32, pid: i32, x: i32, y: i32);
    fn exec_command(&self, command: &str) -> Result<(), String>;
}
```

- `MacOSWindowSystem` / `MacOSWindowManipulator` - Production implementations
- `MockWindowSystem` - Test implementation (`#[cfg(test)]` only)

State methods take `WindowSystem` as parameter:
- `state.sync_all(&window_system)`
- `state.sync_pid(&window_system, pid)`
- `state.handle_event(&window_system, &event)`

### Effect Pattern

Command handling is separated into pure computation and side effects for testability.

**Architecture:**
```rust
// Pure function - returns Response + Effects to execute
fn process_command(
    state: &mut State,
    hotkey_manager: &mut HotkeyManager,
    cmd: &Command,
) -> CommandResult {
    match cmd {
        Command::TagView { tags, output } => {
            let moves = state.view_tags_on_display(*tags, display_id);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::RetileDisplays(vec![display_id]),
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        // Query commands return response with no effects
        Command::ListWindows => {
            CommandResult::with_response(Response::Windows { windows })
        }
        ...
    }
}

// Side effect executor - can use MockWindowManipulator in tests
fn execute_effects<M: WindowManipulator>(
    effects: Vec<Effect>,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
) -> Result<(), String>

// Orchestrator
fn handle_ipc_command<M: WindowManipulator>(...) -> Response {
    let result = process_command(&mut state, &mut hotkey_manager, cmd);
    execute_effects(result.effects, state, layout_engine_manager, manipulator)?;
    result.response
}
```

**Effect enum (`effect.rs`):**
```rust
pub enum Effect {
    ApplyWindowMoves(Vec<WindowMove>),
    FocusWindow { window_id: u32, pid: i32 },
    MoveWindowToPosition { window_id: u32, pid: i32, x: i32, y: i32 },
    Retile,
    RetileDisplays(Vec<DisplayId>),
    SendLayoutCommand { layout: Option<String>, cmd: String, args: Vec<String> },
    ExecCommand(String),
    FocusVisibleWindowIfNeeded,
}
```

**Benefits:**
- `process_command()` is a pure function, fully testable without macOS APIs
- Effects can be inspected/verified in tests
