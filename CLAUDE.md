# Yashiki

macOS tiling window manager written in Rust.

## Project Structure

```
yashiki/          # WM core daemon
yashiki-ipc/      # Shared protocol definitions (commands, layout)
tatami/           # Default tile layout engine (master-stack)
```

Future components:
- `engawa/` - Status bar
- Other layout engines: `rasen` (spiral), `koushi` (grid)

## Architecture

### Thread Model

- **Main thread**: CFRunLoop
  - Accessibility API (AXUIElement, AXObserver)
  - Global hotkeys (CGEventTap)
  - Window operations
- **Tokio runtime** (separate thread):
  - IPC server (Unix Domain Socket)
  - Config file watching
  - State management, layout calculation

### Communication

- macOS → tokio: `tokio::sync::mpsc`
- tokio → macOS: `dispatch::Queue::main().exec_async()`

### Virtual Workspaces (No SIP Required)

Like AeroSpace, uses virtual workspaces instead of macOS native Spaces:
- All windows exist on a single macOS Space
- Workspace switching moves windows off-screen (x = -10000) or shows them
- Only uses public Accessibility API

## Key Features

- **Tag-based workspace management** (like dwm/awesomewm/river)
  - Windows can have multiple tags (bitmask)
  - View any combination of tags
- **External layout engine** (like river)
  - Layout engine is a separate process
  - Communicates via stdin/stdout JSON
  - Users can write custom layout engines

## Layout Protocol

```rust
// yashiki → layout engine
LayoutRequest { output_width, output_height, window_count, main_count, main_ratio }

// layout engine → yashiki
LayoutResponse { windows: Vec<WindowGeometry> }
```

## Implementation Status

### Completed
- `macos/accessibility.rs` - AXUIElement FFI bindings
  - `is_trusted()`, `is_trusted_with_prompt()` - permission check
  - `AXUIElement::application(pid)`, `windows()`, `title()`, `position()`, `size()`
  - `set_position()`, `set_size()` - window manipulation
- `macos/display.rs` - CGWindowList based window enumeration
  - `get_on_screen_windows()` - list on-screen windows
  - `get_running_app_pids()` - get PIDs of apps with windows

### Not Yet Implemented
- Main event loop (CFRunLoop + tokio integration)
- IPC server
- Tag/workspace management
- Layout engine communication
- Config file parsing
- Global hotkeys (CGEventTap)

## Development Notes

- Requires Accessibility permission (System Preferences → Privacy & Security → Accessibility)
- During development, grant permission to the terminal (e.g., Ghostty)
- Run with: `cargo run -p yashiki`

## Dependencies

Key crates:
- `core-foundation` (0.10) - macOS Core Foundation bindings
- `core-graphics` (0.25) - CGWindowList, display info
- `tokio` - async runtime for IPC
- `dispatch` - GCD for main thread communication

## Code Style

- All code in English
- Minimal comments - only where logic is non-obvious
- No unnecessary comments explaining what the next line does
- When adding dependencies, always use the latest version
