# Layout Engine Specification

## Overview

Yashiki uses external layout engines following the river-style approach. Layout engines are separate processes that communicate with yashiki via stdin/stdout JSON messages.

This design allows:
- Custom layout algorithms without modifying yashiki
- Layout engines written in any language
- Independent state management per layout engine

> **Note**: This specification is subject to change during early development.

## Protocol

Communication uses newline-delimited JSON. Each message is a single JSON object followed by a newline.

### Messages from yashiki to layout engine

```rust
enum LayoutMessage {
    // Request layout calculation
    Layout {
        width: u32,      // Available width in pixels
        height: u32,     // Available height in pixels
        windows: Vec<u32> // Window IDs to layout
    },
    // Send command to layout engine
    Command {
        cmd: String,
        args: Vec<String>
    }
}
```

**Example JSON:**
```json
{"Layout":{"width":1920,"height":1080,"windows":[123,456,789]}}
{"Command":{"cmd":"set-main-ratio","args":["0.6"]}}
```

### Messages from layout engine to yashiki

```rust
enum LayoutResult {
    // Layout calculation result
    Layout {
        windows: Vec<WindowGeometry>
    },
    // Command succeeded, no action needed
    Ok,
    // Command succeeded, request retile
    NeedsRetile,
    // Error occurred
    Error {
        message: String
    }
}

struct WindowGeometry {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32
}
```

**Example JSON:**
```json
{"Layout":{"windows":[{"id":123,"x":0,"y":0,"width":960,"height":1080},{"id":456,"x":960,"y":0,"width":960,"height":1080}]}}
{"Ok":null}
{"NeedsRetile":null}
{"Error":{"message":"Invalid ratio value"}}
```

## Focus Notification

Yashiki automatically sends a `focus-changed` command when focus changes:

```json
{"Command":{"cmd":"focus-changed","args":["123"]}}
```

The layout engine should:
1. Track the focused window ID internally
2. Return `Ok` if focus change doesn't affect layout (e.g., tatami)
3. Return `NeedsRetile` if layout depends on focus (e.g., byobu accordion)

## Commands

### Required Commands

| Command | Args | Description |
|---------|------|-------------|
| `focus-changed` | `<window_id>` | Notification of focus change |

### Optional Commands

Layout engines define their own commands. Examples from built-in engines:

**tatami (master-stack):**
- `set-main-ratio <ratio>` - Set main area ratio (0.1-0.9)
- `inc-main-ratio [delta]` - Increase ratio (default: 0.05)
- `dec-main-ratio [delta]` - Decrease ratio
- `inc-main-count` - Increase main window count
- `dec-main-count` - Decrease main window count
- `set-main-count <n>` - Set main window count
- `zoom [window_id]` - Move window to main area
- `set-inner-gap <px>` - Gap between windows
- `set-outer-gap <all>` or `<v h>` or `<t r b l>` - Gap from edges

**byobu (accordion):**
- `set-padding <px>` - Stagger offset between windows
- `set-orientation <horizontal|vertical>` - Stack direction
- `toggle-orientation` - Toggle direction

## Example Implementation

Minimal layout engine in Rust:

```rust
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
enum LayoutMessage {
    Layout { width: u32, height: u32, windows: Vec<u32> },
    Command { cmd: String, args: Vec<String> },
}

#[derive(Serialize)]
enum LayoutResult {
    Layout { windows: Vec<WindowGeometry> },
    Ok,
    NeedsRetile,
    Error { message: String },
}

#[derive(Serialize)]
struct WindowGeometry {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let msg: LayoutMessage = serde_json::from_str(&line).unwrap();

        let result = match msg {
            LayoutMessage::Layout { width, height, windows } => {
                // Simple horizontal split
                let count = windows.len() as u32;
                let w = if count > 0 { width / count } else { width };

                let geometries: Vec<_> = windows.iter().enumerate().map(|(i, &id)| {
                    WindowGeometry {
                        id,
                        x: (i as u32 * w) as i32,
                        y: 0,
                        width: w,
                        height,
                    }
                }).collect();

                LayoutResult::Layout { windows: geometries }
            }
            LayoutMessage::Command { cmd, .. } => {
                match cmd.as_str() {
                    "focus-changed" => LayoutResult::Ok,
                    _ => LayoutResult::Error {
                        message: format!("Unknown command: {}", cmd)
                    },
                }
            }
        };

        serde_json::to_writer(&mut stdout, &result).unwrap();
        writeln!(stdout).unwrap();
        stdout.flush().unwrap();
    }
}
```

## Installation

### Built-in Layouts

Built-in layout engines (`tatami`, `byobu`) are bundled with yashiki.

### Custom Layouts

1. Create an executable named `yashiki-layout-<name>` that implements the protocol
2. Ensure it's discoverable via yashiki's exec path:
   - Built-in layouts are found in the yashiki executable directory
   - System `PATH` is included by default
   - Add custom directories using `add-exec-path` in your init script
3. Use `layout-set` with the layout name (not the full executable name):

```sh
# For a layout engine named yashiki-layout-my-layout
yashiki layout-set my-layout
```

**Exec path management:**

The exec path controls where yashiki searches for layout engines and where `exec` commands run.

```sh
# View current exec path
yashiki exec-path

# Add a directory to the search path (high priority)
yashiki add-exec-path /home/user/my-layouts

# Add to end of search path (low priority)
yashiki add-exec-path --append /opt/yashiki-layouts

# Replace entire exec path
yashiki set-exec-path "/custom/path:/another/path"
```

Default exec path: `<yashiki_executable_dir>:<system_PATH>`

### Configuration Example

```sh
# ~/.config/yashiki/init

# Add custom layout directory to exec path
yashiki add-exec-path /home/user/my-layouts

# Set default layout
yashiki layout-set-default tatami

# Use custom layout for specific tag (layout name only, not path)
yashiki layout-set --tags 4 my-custom-layout

# Configure layout parameters
yashiki layout-cmd --layout tatami set-outer-gap 10
```

## Debugging Tips

1. Test your layout engine standalone:
   ```sh
   echo '{"Layout":{"width":1920,"height":1080,"windows":[1,2,3]}}' | ./my-layout
   ```

2. Check yashiki logs for communication errors:
   ```sh
   RUST_LOG=debug yashiki start
   ```

3. Ensure JSON output is newline-terminated and flushed immediately
