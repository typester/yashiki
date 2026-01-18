# Yashiki (屋敷)

macOS tiling window manager written in Rust.

## Features

- **Tag-based workspaces** - Bitmask tags (like awesome/river) allow windows to belong to multiple tags and view any combination
- **External layout engines** - Stdin/stdout JSON protocol lets you write custom layouts in any language
- **Multi-monitor support** - Each display has independent tags
- **No SIP disable required** - Uses only public Accessibility API
- **Shell script configuration** - Config is just a shell script (`~/.config/yashiki/init`)

## Status

Early development stage. API and configuration format may change.

## Requirements

- macOS 12.0+
- Accessibility permission (System Settings → Privacy & Security → Accessibility)

## Installation

### Homebrew (Recommended)

```sh
brew tap typester/yashiki
brew install --cask yashiki
```

The cask installs:
- `Yashiki.app` to `/Applications`
- CLI tools: `yashiki`, `yashiki-layout-tatami`, `yashiki-layout-byobu`

**Note:** Yashiki.app is not signed. On first launch, allow it in System Settings → Privacy & Security. Or install with `--no-quarantine`:

```sh
brew install --cask --no-quarantine yashiki
```

### From Source

```sh
# Core daemon and CLI
cargo install --path yashiki

# Install layout engines you want to use
cargo install --path yashiki-layout-tatami   # Master-stack layout
cargo install --path yashiki-layout-byobu    # Accordion layout
```

### Grant Accessibility Permission

1. Open System Settings → Privacy & Security → Accessibility
2. Add `Yashiki.app` (if installed via Homebrew or as app bundle)
3. Or add your terminal app if running `yashiki start` directly (Not recommended)

## Quick Start

1. Launch Yashiki.app:
   - If installed via Homebrew: Open `/Applications/Yashiki.app`
   - The app will request Accessibility permission on first launch

   **Note:** Running `yashiki start` from terminal is not recommended as it requires granting Accessibility permission to your terminal app.

2. Create config file `~/.config/yashiki/init`:
   ```sh
   #!/bin/sh

   # Add Homebrew to exec path (if needed for custom layout engines)
   # yashiki add-exec-path /opt/homebrew/bin

   # Layout configuration
   yashiki layout-set-default tatami
   yashiki layout-cmd --layout tatami set-outer-gap 10
   yashiki layout-cmd --layout tatami set-inner-gap 10

   # Tag bindings (tag N = bitmask $((1<<(N-1))))
   for i in 1 2 3 4 5 6 7 8 9; do
     yashiki bind "alt-$i" tag-view "$((1<<(i-1)))"
     yashiki bind "alt-shift-$i" window-move-to-tag "$((1<<(i-1)))"
   done

   # Window focus
   yashiki bind alt-j window-focus next
   yashiki bind alt-k window-focus prev
   yashiki bind alt-h layout-cmd dec-main-ratio
   yashiki bind alt-l layout-cmd inc-main-ratio

   # Multi-monitor
   yashiki bind alt-o output-focus next
   yashiki bind alt-shift-o output-send next
   ```

3. Make it executable:
   ```sh
   chmod +x ~/.config/yashiki/init
   ```

4. Restart yashiki to apply config:
   - Quit with `yashiki quit`
   - Relaunch Yashiki.app

## Configuration

Yashiki uses a shell script for configuration. The init script is executed when the daemon starts.

### Hotkey Syntax

Format: `<modifiers>-<key>`

**Modifiers:**
- `alt` (Option key)
- `ctrl` (Control key)
- `shift`
- `cmd` (Command key)

**Examples:** `alt-1`, `alt-shift-j`, `ctrl-alt-return`

### Tag System

Tags use bitmask format:
- Tag 1 = `1` (binary: 001)
- Tag 2 = `2` (binary: 010)
- Tag 3 = `4` (binary: 100)
- Tags 1+2 = `3` (binary: 011)

In shell scripts: `$((1<<0))` = 1, `$((1<<1))` = 2, `$((1<<2))` = 4

## CLI Commands

### Daemon Control

```sh
yashiki start              # Start daemon
yashiki quit               # Stop daemon
yashiki version            # Show version
```

### Hotkey Management

```sh
yashiki bind alt-1 tag-view 1    # Bind hotkey
yashiki unbind alt-1             # Unbind hotkey
yashiki list-bindings            # List all bindings
```

### Tag Operations

```sh
yashiki tag-view 1               # Switch to tag 1
yashiki tag-view 3               # View tags 1+2 (bitmask 3)
yashiki tag-toggle 2             # Toggle tag 2 visibility
yashiki tag-view-last            # Switch to previous tags
yashiki window-move-to-tag 1     # Move focused window to tag 1
yashiki window-toggle-tag 2      # Toggle tag 2 on focused window
```

### Window Focus

```sh
yashiki window-focus next        # Focus next window
yashiki window-focus prev        # Focus previous window
yashiki window-focus left        # Focus window to the left
yashiki window-focus right       # Focus window to the right
yashiki window-focus up          # Focus window above
yashiki window-focus down        # Focus window below
```

### Multi-Monitor

```sh
yashiki output-focus next        # Focus next display
yashiki output-focus prev        # Focus previous display
yashiki output-send next         # Move window to next display
yashiki output-send prev         # Move window to previous display
yashiki tag-view --output 2 1    # Switch tag on display 2
yashiki tag-view --output "DELL" 1  # Target display by name
```

### Layout

```sh
yashiki retile                   # Apply layout
yashiki layout-set-default tatami     # Set default layout
yashiki layout-set byobu              # Set layout for current tag
yashiki layout-set --tags 4 byobu     # Set layout for tag 3
yashiki layout-get                    # Get current layout
yashiki layout-cmd set-main-ratio 0.6 # Send command to layout
yashiki layout-cmd --layout tatami set-outer-gap 10  # Configure specific layout
```

### Utilities

```sh
yashiki list-windows             # List all windows
yashiki list-outputs             # List all displays
yashiki get-state                # Get current state
yashiki exec "open -a Safari"    # Execute command
yashiki exec-or-focus --app-name Safari "open -a Safari"  # Focus or launch
```

### Exec Path

The exec path is used for `exec` commands and custom layout engine discovery.

```sh
yashiki exec-path                # Get current exec path
yashiki set-exec-path "/path1:/path2"  # Set exec path
yashiki add-exec-path /opt/homebrew/bin       # Add to start (high priority)
yashiki add-exec-path --append /usr/local/bin # Add to end (low priority)
```

Default exec path: `<yashiki_executable_dir>:<system_PATH>`

## Built-in Layout Engines

### tatami (master-stack)

Classic tiling layout with main area and stack.

**Commands:**
| Command | Description |
|---------|-------------|
| `set-main-ratio <0.1-0.9>` | Set main area ratio |
| `inc-main-ratio` | Increase main ratio |
| `dec-main-ratio` | Decrease main ratio |
| `inc-main-count` | Add window to main area |
| `dec-main-count` | Remove window from main area |
| `zoom [window_id]` | Move window to main area |
| `set-inner-gap <px>` | Gap between windows |
| `set-outer-gap <px>` | Gap from screen edges |

### byobu (accordion)

AeroSpace-style stacked windows with focused window at front.

**Commands:**
| Command | Description |
|---------|-------------|
| `set-padding <px>` | Stagger offset between windows |
| `set-orientation <h\|v>` | Horizontal or vertical stacking |
| `toggle-orientation` | Toggle orientation |

## Custom Layout Engines

Yashiki supports external layout engines via stdin/stdout JSON protocol.

See [docs/layout-engine.md](docs/layout-engine.md) for the specification.

## Development

```sh
# Run daemon with debug logging
RUST_LOG=info cargo run -p yashiki -- start

# Run CLI commands
cargo run -p yashiki -- list-windows

# Run tests
cargo test --all

# Format code
cargo fmt --all
```

## Project Structure

```
yashiki/                  # WM core daemon + CLI
yashiki-ipc/              # Shared protocol definitions
yashiki-layout-tatami/    # Master-stack layout engine
yashiki-layout-byobu/     # Accordion layout engine
```

## Acknowledgments

Inspired by:
- [river](https://codeberg.org/river/river) - External layout protocol, multi-monitor model
- [AeroSpace](https://github.com/nikitabobko/AeroSpace) - Virtual workspaces approach, accordion layout
- [dwm](https://dwm.suckless.org/) / [awesomewm](https://awesomewm.org/) - Tag-based workspaces

## License

MIT License - see [LICENSE](LICENSE) for details.
