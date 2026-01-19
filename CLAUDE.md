# Yashiki

macOS tiling window manager written in Rust.

## Workspace Notes

- Version control: `jj` (not `git`)
- Before starting work:
  1. Run `jj workspace root` to get the workspace root path
  2. Run `jj status` to confirm current workspace state
- All file edits must target files under the workspace root path
- Update only this CLAUDE.md, not the root one

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
- Hotkey commands: CGEventTap callback → main thread via `std::sync::mpsc` + `CFRunLoopSource`
  - Commands are queued via mpsc channel
  - `CFRunLoopSourceSignal` triggers immediate processing (no `CFRunLoopWakeUp` needed - same thread)
  - No polling delay for hotkey command processing
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
- **Window rules** (riverctl-style)
  - Automatically configure window properties based on app name, bundle identifier (app-id), or title
  - Glob pattern matching (`*Chrome*`, `Safari`, `*Dialog*`, `com.apple.*`)
  - Actions: ignore, float, no-float, tags, output, position, dimensions
  - Rules sorted by specificity (more specific rules take priority)
- **Cursor warp** (mouse follows focus)
  - Similar to river's `set-cursor-warp`
  - Three modes: `disabled`, `on-output-change`, `on-focus-change`
  - When enabled, mouse cursor moves to window center on focus change
- **State streaming** (for status bars like engawa)
  - Real-time state change events via Unix socket (`/tmp/yashiki-events.sock`)
  - Event types: window (created/destroyed/updated), focus, display, tags, layout
  - Optional snapshot on connection
  - Filtering by event type

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

## State Streaming Protocol

State events are streamed via Unix socket `/tmp/yashiki-events.sock`.

```rust
// yashiki-ipc/src/event.rs

// Client sends on connection
struct SubscribeRequest {
    snapshot: bool,           // Request initial snapshot
    filter: EventFilter,      // Filter events (empty = all)
}

struct EventFilter {
    window: bool,   // WindowCreated, WindowDestroyed, WindowUpdated
    focus: bool,    // WindowFocused, DisplayFocused
    display: bool,  // DisplayAdded, DisplayRemoved, DisplayUpdated
    tags: bool,     // TagsChanged
    layout: bool,   // LayoutChanged
}

// Server streams events (JSON lines)
enum StateEvent {
    WindowCreated { window: WindowInfo },
    WindowDestroyed { window_id: u32 },
    WindowUpdated { window: WindowInfo },
    WindowFocused { window_id: Option<u32> },
    DisplayFocused { display_id: u32 },
    DisplayAdded { display: OutputInfo },
    DisplayRemoved { display_id: u32 },
    DisplayUpdated { display: OutputInfo },
    TagsChanged { display_id: u32, visible_tags: u32, previous_tags: u32 },
    LayoutChanged { display_id: u32, layout: String },
    Snapshot { windows, displays, focused_window_id, focused_display_id, default_layout },
}
```

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
yashiki window-swap next          # Swap with next window
yashiki window-swap prev          # Swap with previous window
yashiki window-swap left          # Swap with window to the left
yashiki window-swap right         # Swap with window to the right
yashiki window-swap up            # Swap with window above
yashiki window-swap down          # Swap with window below
yashiki window-toggle-fullscreen  # Toggle fullscreen for focused window (AeroSpace-style)
yashiki window-toggle-float       # Toggle floating state for focused window
yashiki window-close              # Close the focused window
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
yashiki layout-cmd --layout tatami set-inner-gap 10  # Send command to specific layout
yashiki layout-cmd inc-main-count       # Increase main window count
yashiki layout-cmd zoom                 # Move focused window to main area (tatami)
yashiki layout-cmd zoom 123             # Move specific window to main area (tatami)
yashiki list-windows              # List all windows
yashiki list-outputs              # List all displays/outputs
yashiki get-state                 # Get current state
yashiki exec "open -a Safari"     # Execute shell command
yashiki exec-or-focus --app-name Safari "open -a Safari"  # Focus if running, else exec
yashiki exec-path                 # Get current exec path
yashiki set-exec-path "/opt/homebrew/bin:/usr/local/bin"  # Set exec path
yashiki add-exec-path /opt/homebrew/bin      # Add to start of exec path (high priority)
yashiki add-exec-path --append /usr/local/bin  # Add to end of exec path (low priority)
yashiki rule-add --app-name Safari tags $((1<<1)) # Safari windows go to tag 2 (bitmask 2)
yashiki rule-add --app-name Finder float          # Finder windows float
yashiki rule-add --title "*Dialog*" float         # Windows with "Dialog" in title float
yashiki rule-add --app-name Safari --title "*Preferences*" float  # More specific rule
yashiki rule-add --app-name Preview dimensions 800 600  # Set initial size
yashiki rule-add --app-name "Google Chrome" output 2    # Chrome to display 2
yashiki rule-add --app-id com.apple.finder float  # Match by bundle identifier
yashiki rule-add --app-id "com.google.*" output 2 # Glob pattern for bundle ID
yashiki rule-add --ax-id "com.mitchellh.ghostty.quickTerminal" float  # Match by AXIdentifier
yashiki rule-add --subrole Dialog float           # Match by AXSubrole (AX prefix optional)
yashiki rule-add --subrole AXUnknown ignore       # Ignore popup windows (never manage)
yashiki rule-add --window-level other ignore      # Ignore non-normal windows (palettes, etc.)
yashiki rule-add --window-level floating float    # Float utility panels (level 3)
yashiki rule-add --fullscreen-button none float   # Float windows without fullscreen button
yashiki rule-add --close-button none ignore       # Ignore windows without close button (popups)
yashiki rule-add --app-id com.mitchellh.ghostty --fullscreen-button disabled ignore  # Ghostty Quick Terminal
yashiki rule-del --app-name Finder float          # Remove a rule
yashiki list-rules                # List all rules
yashiki set-cursor-warp disabled          # Disable cursor warp (default)
yashiki set-cursor-warp on-output-change  # Warp on display switch only
yashiki set-cursor-warp on-focus-change   # Warp on all focus changes
yashiki get-cursor-warp           # Get current cursor warp mode
yashiki set-outer-gap 10              # Set outer gap (all sides)
yashiki set-outer-gap 10 20           # Set outer gap (vertical, horizontal)
yashiki set-outer-gap 10 20 15 25     # Set outer gap (top, right, bottom, left)
yashiki get-outer-gap                 # Get current outer gap
yashiki subscribe                 # Subscribe to all state events
yashiki subscribe --snapshot      # Subscribe with initial snapshot
yashiki subscribe --filter focus,tags  # Subscribe to specific events
yashiki quit                      # Quit daemon
```

## Config Example

```sh
# ~/.config/yashiki/init
#!/bin/sh

# Extend exec path (for layout engines and exec commands)
yashiki add-exec-path /opt/homebrew/bin

# Cursor warp (mouse follows focus)
yashiki set-cursor-warp on-focus-change

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
yashiki bind alt-f window-toggle-fullscreen
yashiki bind alt-shift-f window-toggle-float
yashiki bind alt-shift-c window-close

# Gap configuration
yashiki set-outer-gap 10  # Outer gap: applied by daemon to all layouts + fullscreen
yashiki layout-cmd --layout tatami set-inner-gap 10  # Inner gap: layout-specific
yashiki layout-cmd --layout byobu set-padding 30  # Byobu stagger offset

# Window rules (applied to new windows automatically)
yashiki rule-add --app-name Finder float          # Finder windows float
yashiki rule-add --app-name "System Preferences" float
yashiki rule-add --title "*Dialog*" float         # Dialog windows float
yashiki rule-add --app-name Safari tags $((1<<1)) # Safari goes to tag 2 (bitmask 2)
yashiki rule-add --app-name "Google Chrome" output 2  # Chrome to external display
yashiki rule-add --app-id com.apple.Preview float # Match by bundle identifier
yashiki rule-add --app-id "com.electron.*" float  # Electron apps float

# App launchers
yashiki bind alt-return exec "open -n /Applications/Ghostty.app"
yashiki bind alt-s exec-or-focus --app-name Safari "open -a Safari"
```

## Implementation Status

### Completed
- **macos/accessibility.rs** - AXUIElement FFI bindings
  - Permission check, window manipulation (position, size), `raise()` for focus
- **macos/display.rs** - CGWindowList window enumeration, display info
  - `get_on_screen_windows()` (includes bundle_id), `get_all_displays()` (uses NSScreen visibleFrame)
- **macos/observer.rs** - AXObserver for window events
  - `ObserverManager` with `add_observer()`, `remove_observer()`, `has_observer()`
- **macos/workspace.rs** - NSWorkspace app launch/terminate notifications, display change notifications, `activate_application()`, `get_frontmost_app_pid()`, `get_bundle_id_for_pid()`, `exec_command()`
- **macos/hotkey.rs** - CGEventTap global hotkeys
  - `HotkeyManager` with dynamic bind/unbind
  - Deferred tap recreation via dirty flag (batches multiple bind/unbind calls)
  - Signals CFRunLoopSource for immediate command processing
- **core/display.rs** - Display struct with visible_tags per display
- **core/state.rs** - Window and display state management
  - Multi-monitor: `displays`, `focused_display`, per-display visible_tags
  - Tag operations: `view_tags()`, `toggle_tags_on_display()`, `move_focused_to_tags()`, `toggle_focused_window_tags()`
  - Focus: `focus_window()` - stack-based (next/prev) and geometry-based (left/right/up/down)
  - Output: `focus_output()`, `send_to_output()` - move focus/window between displays
  - Display targeting: `resolve_output()`, `get_target_display()` - resolve OutputSpecifier to DisplayId
  - Display change: `handle_display_change()` - handle monitor connect/disconnect
  - Window rules: `add_rule()`, `remove_rule()`, `should_ignore_window()`, `apply_rules_to_new_window()`
- **core/window.rs** - Window struct with tags, display_id, app_id, ax_id, subrole, window_level, button states (close, fullscreen, minimize, zoom), saved_frame, is_floating, is_fullscreen
- **core/tag.rs** - Tag bitmask
- **ipc/server.rs** - IPC server on `/tmp/yashiki.sock`
- **ipc/client.rs** - IPC client for CLI and event subscription
- **ipc/event_server.rs** - Event streaming server on `/tmp/yashiki-events.sock`
  - Broadcast channel for multiple subscribers
  - Event filtering per connection
- **event_emitter.rs** - Main thread to tokio event forwarding
- **layout.rs** - `LayoutEngine` and `LayoutEngineManager`
  - `LayoutEngine` spawns and communicates with a single layout engine process
  - `LayoutEngineManager` manages multiple engines with lazy spawning
- **app.rs** - Main event loop with CFRunLoop
  - CFRunLoopSource for immediate IPC command processing
  - CFRunLoopSource for immediate hotkey command processing
  - CFRunLoop timer (50ms) for workspace events, observer events, and deferred hotkey tap updates
  - Periodic window scanning for apps without observers (e.g., Finder with only desktop at startup)
  - Auto-retile on window add/remove
  - Runs init script at startup
  - Effect pattern: `process_command()` (pure) + `execute_effects()` (side effects)
- **effect.rs** - Effect enum and CommandResult for separating pure computation from side effects
- **platform.rs** - Platform abstraction layer
  - `WindowSystem` trait for querying window/display info (`get_extended_attributes()` for window_level and button states)
  - `WindowManipulator` trait for window manipulation side effects
  - `MacOSWindowSystem` / `MacOSWindowManipulator` - Production implementations
  - `MockWindowSystem` / `MockWindowManipulator` - Test implementations
- **main.rs** - Daemon + CLI mode
- **yashiki-ipc/** - Command/Response/LayoutMessage enums, OutputSpecifier, OutputInfo, GlobPattern, RuleMatcher, RuleAction, WindowRule, StateEvent, SubscribeRequest, EventFilter, ButtonState, WindowLevel, ButtonInfo, ExtendedWindowAttributes

### yashiki-layout-tatami (layout engine)
- Master-stack layout
- Internal state: main_count, main_ratio, inner_gap, focused_window_id, main_window_id
- Commands:
  - `focus-changed <window_id>` - notification from yashiki (returns `Ok`)
  - `zoom [window_id]` - set main window (uses focused window if id omitted)
  - `set-main-ratio <0.1-0.9>`, `inc-main-ratio [delta]`, `dec-main-ratio [delta]` (default delta: 0.05)
  - `inc-main-count`, `dec-main-count`, `set-main-count <n>`
  - `set-inner-gap <px>` - gap between windows
  - `inc-inner-gap [delta]`, `dec-inner-gap [delta]`

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

### Use Statement Ordering

Use statements should be ordered in three groups with blank lines between them:

1. **std crates** - Standard library (`std::`, `core::`, `alloc::`)
2. **external crates** - Third-party crates (anyhow, tokio, core_foundation, etc.)
3. **internal crates** - Project modules (`crate::`, `super::`, `self::`, `yashiki_ipc`)

Each group should be sorted alphabetically. Module declarations (`mod`, `pub mod`) come before use statements.

## Workflow

- When user asks to plan something, present the plan first and wait for approval before implementing
- Do not start implementation until user confirms the plan
- Run `cargo fmt --all` at the end of each task
- Update documentation when adding/changing features:
  - `README.md` - User-facing documentation (features, CLI usage, examples)
  - `CLAUDE.md` - Developer documentation (architecture, implementation details, test coverage)

## Design Decisions

### Hotkey Dynamic Update
- Bindings stored in `HashMap<Hotkey, Command>` on main thread
- `bind()`/`unbind()` sets `dirty = true` without recreating tap
- `ensure_tap()` called in timer callback (50ms interval) recreates tap only if dirty
- This batches multiple bind/unbind calls during init script execution
- No Mutex needed - tap callback gets owned clone of bindings

### Hotkey Immediate Processing
- CGEventTap callback signals CFRunLoopSource after sending command to channel
- CFRunLoopSource callback processes commands immediately (no polling delay)
- Only `CFRunLoopSourceSignal` is needed (no `CFRunLoopWakeUp`) because CGEventTap runs on main thread
- `HotkeyManager` holds `Arc<AtomicPtr<c_void>>` to share source pointer with tap callback
- Source pointer is set after CFRunLoopSource is created and registered

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

### Cursor Warp (Mouse Follows Focus)
- Similar to river's `set-cursor-warp`
- Three modes controlled by `State.cursor_warp: CursorWarpMode`
  | Mode | Behavior |
  |------|----------|
  | `Disabled` (default) | Cursor never moves on focus change |
  | `OnOutputChange` | Cursor moves only when switching displays (output-focus) |
  | `OnFocusChange` | Cursor moves on all focus changes |
- Uses `CGWarpMouseCursorPosition` to move cursor to window center
- `Effect::FocusWindow` includes `is_output_change: bool` to distinguish output changes

### Window Rules
- Rules stored in `State.rules: Vec<WindowRule>`
- Rules sorted by specificity (more specific rules first)
- Matching options:
  - `--app-name` (app name), `--app-id` (bundle identifier), `--title` (window title)
  - `--ax-id` (AXIdentifier), `--subrole` (AXSubrole)
  - `--window-level` (CGWindowLevel: normal, floating, modal, utility, popup, other, or numeric)
  - `--close-button`, `--fullscreen-button`, `--minimize-button`, `--zoom-button` (button states: exists, none, enabled, disabled)
- For subrole matching, "AX" prefix is optional: `--subrole Dialog` matches `AXDialog`
- Specificity calculation: exact match > prefix/suffix > contains > wildcard; button matchers add fixed specificity
- Multiple rules can match; each action type uses "first match wins"
- Floating windows excluded from tiling (`visible_windows_on_display()` filter)
- Rules applied in `timer_callback` after `sync_pid()` returns new window IDs
- Actions:
  | Action | Effect |
  |--------|--------|
  | `ignore` | Skip window completely (never manage) - checked in `sync_pid()` before Window creation |
  | `float` | Set `window.is_floating = true`, excluded from tiling |
  | `no-float` | Set `window.is_floating = false` (override more general float rule) |
  | `tags N` | Set `window.tags = N` |
  | `output N` | Set `window.display_id` to resolved display |
  | `position X Y` | Move window to position (immediate effect) |
  | `dimensions W H` | Resize window (immediate effect) |

### Outer Gap (Global vs Per-Layout)
- Outer gap is managed by yashiki daemon, not layout engines
- Applied uniformly to ALL layouts (tatami, byobu, custom engines)
- Applied to fullscreen windows as well
- State: `State.outer_gap: OuterGap`
- Implementation: yashiki subtracts outer gap from dimensions before sending to layout engines, then adds offset when applying geometries
- CSS-style syntax: `<all>` | `<v h>` | `<t r b l>`

### Popup Window Filtering (Configurable via Rules)
- Problem: Some apps (Firefox, etc.) create temporary popup windows (dropdowns, tooltips) that trigger layout recalculation
- Solution: Use `ignore` rule action to skip specific windows based on AX attributes
- `sync_pid()` checks `should_ignore_window()` before creating Window objects
- Debug logging: `RUST_LOG=yashiki=debug` shows all discovered windows with their AX attributes
- Example rules:
  ```sh
  # Ignore all AXUnknown windows (Firefox dropdowns, tooltips, etc.)
  yashiki rule-add --subrole AXUnknown ignore

  # Ignore only Firefox popup windows
  yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore
  ```

## Testing

### Current Test Coverage

Run tests: `cargo test --all`

**Tested modules:**
- `core/tag.rs` - Tag bitmask operations (7 tests)
- `macos/hotkey.rs` - `parse_hotkey()`, `format_hotkey()` (15 tests)
- `yashiki-ipc` - Command/Response/LayoutMessage/WindowRule/StateEvent/OuterGap serialization
- `core/state.rs` - State management with MockWindowSystem (19 tests)
- `app.rs` - `process_command()` effect generation, `emit_state_change_events()` event detection (15 tests)
- `event_emitter.rs` - `create_snapshot()`, `window_to_info()`, `display_to_info()` (3 tests)
- `yashiki-layout-byobu` - Accordion layout and commands (9 tests)

### Platform Abstraction Layer

`platform.rs` provides traits for testability:

```rust
// For querying window/display information
pub trait WindowSystem {
    fn get_on_screen_windows(&self) -> Vec<WindowInfo>;
    fn get_all_displays(&self) -> Vec<DisplayInfo>;
    fn get_focused_window(&self) -> Option<FocusedWindowInfo>;
    fn get_ax_attributes(&self, window_id: u32, pid: i32) -> (Option<String>, Option<String>);
}

// For window manipulation side effects
pub trait WindowManipulator {
    fn apply_window_moves(&self, moves: &[WindowMove]);
    fn apply_layout(&self, display_id: DisplayId, frame: &Rect, geometries: &[WindowGeometry]);
    fn focus_window(&self, window_id: u32, pid: i32);
    fn move_window_to_position(&self, window_id: u32, pid: i32, x: i32, y: i32);
    fn set_window_dimensions(&self, window_id: u32, pid: i32, width: u32, height: u32);
    fn exec_command(&self, command: &str, path: &str) -> Result<(), String>;
    fn warp_cursor(&self, x: i32, y: i32);
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
    FocusWindow { window_id: u32, pid: i32, is_output_change: bool },
    MoveWindowToPosition { window_id: u32, pid: i32, x: i32, y: i32 },
    SetWindowDimensions { window_id: u32, pid: i32, width: u32, height: u32 },
    Retile,
    RetileDisplays(Vec<DisplayId>),
    SendLayoutCommand { layout: Option<String>, cmd: String, args: Vec<String> },
    ExecCommand { command: String, path: String },
    UpdateLayoutExecPath { path: String },
    FocusVisibleWindowIfNeeded,
}
```

**Benefits:**
- `process_command()` is a pure function, fully testable without macOS APIs
- Effects can be inspected/verified in tests
