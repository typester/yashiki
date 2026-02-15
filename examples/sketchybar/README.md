# Yashiki + SketchyBar Example (Minimal)

A minimal workspace indicator for [SketchyBar](https://github.com/FelixKratz/SketchyBar), showing tags 1-10 with active/occupied/vacant states.

This example prioritizes simplicity and readability. It does not handle multi-monitor setups or process management. For a more complete example, see [sketchybar-menubar](../sketchybar-menubar/).

## Prerequisites

- [SketchyBar](https://github.com/FelixKratz/SketchyBar)
- [Hack Nerd Font](https://github.com/ryanoasis/nerd-fonts) (`brew install --cask font-hack-nerd-font`)
- [jq](https://jqlang.github.io/jq/) (`brew install jq`)
- yashiki running

## Usage

Copy this directory to `~/.config/sketchybar/` and restart SketchyBar:

```sh
cp -r examples/sketchybar/* ~/.config/sketchybar/
brew services restart sketchybar
```

## How It Works

### Architecture

```
yashiki subscribe --> yashiki_bridge.sh --> sketchybar --trigger
                                                |
                                          workspace.sh (per tag)
```

1. **sketchybarrc** - Configures a 40px top bar with 10 tag indicators and starts the event bridge
2. **plugins/yashiki_bridge.sh** - Subscribes to yashiki events, maintains workspace state with jq, and triggers SketchyBar updates
3. **plugins/workspace.sh** - Called per tag on each update, sets visual state based on bitmask comparison

### Tag States

| State | Meaning | Appearance |
|-------|---------|------------|
| Active | Visible on focused display | White icon + background |
| Occupied | Has windows but not visible | White icon |
| Vacant | No windows | Dimmed icon |

Clicking a tag runs `yashiki tag-view <bitmask>` to switch to it.

### Limitations

- **Single display only** - Shows state of the focused display; does not track other displays
- **No process management** - The bridge runs as a bare background process; if it crashes, restart SketchyBar
