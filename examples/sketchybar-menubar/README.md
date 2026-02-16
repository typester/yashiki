# Yashiki + SketchyBar Example (Menubar Overlay, Multi-Monitor)

A workspace indicator that overlays the macOS menu bar, supporting multiple displays with per-display tag indicators. Unfocused displays are shown dimmed.

<img src="screenshot.png" width="800">

For a simpler single-display example, see [sketchybar](../sketchybar/).

## Prerequisites

- [SketchyBar](https://github.com/FelixKratz/SketchyBar) (`brew install felixkratz/formulae/sketchybar`)
- [Hack Nerd Font](https://github.com/ryanoasis/nerd-fonts) (`brew install --cask font-hack-nerd-font`)
- [reap](https://github.com/typester/reap) (`brew install typester/reap/reap`)
- python3 (macOS built-in)
- yashiki running

## Usage

Add to your `~/.config/yashiki/init`:

```sh
yashiki exec --track "sketchybar -c path/to/examples/sketchybar-menubar/sketchybarrc"
```

This starts SketchyBar alongside yashiki and automatically terminates it on `yashiki quit`.

Or run standalone:

```sh
sketchybar -c path/to/examples/sketchybar-menubar/sketchybarrc
```

## Known Limitations

### Menu Bar Click-Through

SketchyBar overlays the macOS menu bar as a topmost window. While SketchyBar's transparent areas allow clicks to pass through to regular windows, they do **not** pass through to the macOS menu bar (which is a system UI layer, not a regular window). This means OS menu bar items (app menus, Wi-Fi, clock, etc.) behind the SketchyBar overlay cannot be clicked.

**Workaround:** Bind a hotkey to toggle SketchyBar visibility:

```sh
yashiki bind alt-b exec "sketchybar --bar hidden=toggle"
```

Press the hotkey to hide SketchyBar when you need to access menu bar items, then press again to show it.

## Configuration

Edit the constants at the top of `plugins/yashiki_bridge.py` and the matching values in `sketchybarrc`:

| Variable | Default | Description |
|----------|---------|-------------|
| `MAX_DISPLAYS` | `3` | Maximum number of displays to support (must match `sketchybarrc`) |
| `NUM_TAGS` | `10` | Number of workspace tags (must match `sketchybarrc`) |
| `NOTCH_DISPLAY_ID` | `"1"` | Yashiki display ID of the notched display. Set to `None` for desktop Macs without a notch |
| `BASE_DISPLAY_WIDTH` | `1512` | Reference display width (points) for scaling calculation |
| `BASE_NOTCH_WIDTH` | `200` | Notch width (points) at reference display width |
| `BASE_BAR_HEIGHT` | `25` | Bar height (points) at reference display width |
| `BASE_BAR_Y_OFFSET` | `5` | Bar vertical offset (points) at reference display width |

To find your display IDs, run `yashiki list-outputs`.

### Notch Display

The notched display's tags are positioned at the right end of the menu bar (between system tray and the notch). Other displays use center positioning. Set `NOTCH_DISPLAY_ID = None` if none of your displays have a notch.

### Bar Scaling

The builtin display's resolution (in points) may change depending on the connected external displays and their scaling settings. For example, on one setup a MacBook Pro 14" reports 1512pt with an external display connected, but 1800pt without one. When this happens, the notch width, bar height, and vertical offset need to be adjusted accordingly.

The bridge automatically scales `notch_width`, `height`, and `y_offset` based on the ratio of the current display width to `BASE_DISPLAY_WIDTH`. The defaults are calibrated for a MacBook Pro 14" (M1/M2/M3).

If the bar position doesn't look right on your machine, adjust the base constants:

1. Run `sketchybar --query displays` and note the `frame.w` value of your notched display
2. Set `BASE_DISPLAY_WIDTH` to that value
3. Adjust `BASE_NOTCH_WIDTH` until the tags align with the right edge of the notch
4. Adjust `BASE_BAR_HEIGHT` and `BASE_BAR_Y_OFFSET` if the bar height or vertical position looks off

## How It Works

### Architecture

```
yashiki subscribe --> yashiki_bridge.py --> sketchybar --set (direct updates)
```

Unlike the minimal example, this bridge directly updates SketchyBar item properties via `--set` commands instead of using event triggers.

1. **sketchybarrc** - Configures a 25px transparent overlay bar with per-display tag groups (brackets), starts the bridge via `reap`
2. **plugins/yashiki_bridge.py** - Subscribes to yashiki events, manages display slot assignment, and updates all SketchyBar items directly

### Multi-Monitor Support

The bridge maintains a **slot assignment** that maps yashiki display IDs to SketchyBar display slots (1-3). This mapping is independent of SketchyBar's internal arrangement IDs, so displays can be connected/disconnected without restarting SketchyBar.

- On display connect: new display is assigned the lowest free slot
- On display disconnect: slot is freed for reuse
- Each slot's click handler targets the correct yashiki display via `yashiki tag-view --output <display_id>`

### Tag States

| State | Focused Display | Unfocused Display |
|-------|----------------|-------------------|
| Active | Bright icon + background | Dimmed icon + background |
| Occupied | Bright icon | Dimmed icon |
| Vacant | Gray icon | Dark gray icon |

### Process Management

The bridge is started via [reap](https://github.com/nickolasburr/reap) with `--watch $PPID`, which automatically terminates the bridge when SketchyBar exits.

The bridge also kills any old instances of itself on startup to prevent duplicates.
