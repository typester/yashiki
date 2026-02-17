# Yashiki + Ranma Example (Workspace Indicator)

A workspace indicator for [Ranma](https://github.com/typester/ranma), displaying workspace tags (1-10) per display with visual states for active, occupied, and vacant workspaces. Updates in real-time via yashiki event subscription. Supports multiple displays with unfocused displays shown dimmed.

<img src="screenshot.png" width="800">

Unlike SketchyBar overlays, Ranma places dynamically-sized floating windows on the menu bar that cover only their content, so click-through works correctly on uncovered regions including macOS menu bar items.

The actual widget code lives in the [ranma repository](https://github.com/typester/ranma/tree/main/examples/yashiki-workspace).

## Prerequisites

- [Ranma](https://github.com/typester/ranma) (`brew install typester/ranma/ranma`)
- [reap](https://github.com/typester/reap) (`brew install typester/reap/reap`)
- [Hack Nerd Font](https://github.com/ryanoasis/nerd-fonts) (`brew install --cask font-hack-nerd-font`)
- python3 (macOS built-in)
- yashiki running

## Usage

Add to your `~/.config/yashiki/init`:

```sh
yashiki exec --track "ranma-server start --init /path/to/ranma/examples/yashiki-workspace/init"
```

This starts Ranma alongside yashiki and automatically terminates it on `yashiki quit`.

If installed via Homebrew, the example is bundled at:

```sh
yashiki exec --track "ranma-server start --init $(brew --prefix)/share/ranma/examples/yashiki-workspace/init"
```

## Configuration

Edit `config.json` in the ranma example directory:

| Key | Default | Description |
|-----|---------|-------------|
| `accent_color` | `#ffffffcc` | Accent color for active tag indicators |
| `font_family` | `Hack Nerd Font` | Font for tag labels |

## How It Works

### Architecture

```
yashiki subscribe --> yashiki_bridge.py --> ranma set (direct updates)
```

1. **init** - Starts the bridge process via `reap`
2. **plugins/yashiki_bridge.py** - Subscribes to yashiki events, manages per-display slot assignment, and updates ranma items directly

### Multi-Monitor Support

The bridge maintains a slot assignment mapping yashiki display IDs to ranma display slots. Displays can be connected/disconnected without restarting. Each slot's click handler targets the correct yashiki display via `yashiki tag-view --output <display_id>`.

### Tag States

| State | Focused Display | Unfocused Display |
|-------|----------------|-------------------|
| Active | Bright label + pill background + underline bar | Dimmed label + pill + bar |
| Occupied | Bright label + dot indicator | Dimmed label + dot |
| Vacant | Gray label | Dark gray label |

Clicking a tag runs `yashiki tag-view <bitmask> --output <display_id>` to switch to it.
