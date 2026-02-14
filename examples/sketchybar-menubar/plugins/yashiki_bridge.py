#!/usr/bin/env python3

"""Multi-monitor event bridge between yashiki and SketchyBar.

Subscribes to yashiki state events, tracks per-display workspace state,
and directly updates SketchyBar item properties via --set commands.
Supports up to 3 displays. Unfocused displays are shown dimmed.

Slot assignment is managed by the bridge independently of SketchyBar's
arrangement-id, so display connect/disconnect is handled correctly
without requiring a SketchyBar restart.

Runs as a long-lived background process, started from sketchybarrc.
"""

import json
import os
import signal
import subprocess
import sys
import time

MAX_DISPLAYS = 3
NUM_TAGS = 10
# Yashiki display ID of the notched display (None if no notch, e.g. desktop Mac)
NOTCH_DISPLAY_ID = "1"

# Colors for focused display
FOCUSED_ACTIVE_ICON = "0xffffffff"
FOCUSED_ACTIVE_BG = "0x40ffffff"
FOCUSED_OCCUPIED_ICON = "0xffffffff"
FOCUSED_VACANT_ICON = "0xff888888"
FOCUSED_BRACKET_BG = "0x80000000"

# Colors for unfocused display
UNFOCUSED_ACTIVE_ICON = "0x80ffffff"
UNFOCUSED_ACTIVE_BG = "0x20ffffff"
UNFOCUSED_OCCUPIED_ICON = "0x80ffffff"
UNFOCUSED_VACANT_ICON = "0x40888888"
UNFOCUSED_BRACKET_BG = "0x40000000"


def kill_old_instances():
    """Kill any existing yashiki_bridge.py processes (except ourselves)."""
    my_pid = os.getpid()
    try:
        result = subprocess.run(
            ["pgrep", "-f", "yashiki_bridge.py"],
            capture_output=True,
            text=True,
        )
        for line in result.stdout.strip().split("\n"):
            if not line:
                continue
            pid = int(line)
            if pid != my_pid:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
    except Exception:
        pass


def query_sketchybar_displays():
    """Query SketchyBar for DirectDisplayID -> arrangement-id mapping."""
    try:
        result = subprocess.run(
            ["sketchybar", "--query", "displays"],
            capture_output=True,
            text=True,
        )
        displays = json.loads(result.stdout)
        mapping = {}
        for d in displays:
            did = str(d["DirectDisplayID"])
            mapping[did] = d["arrangement-id"]
        return mapping
    except Exception:
        return {}


def assign_slots(state):
    """Assign display slots (1-3) to yashiki displays.

    Maintains existing assignments. New displays get the lowest free slot.
    Removed displays free their slot.
    """
    current_displays = set(state["displays"].keys())
    assignment = state["slot_assignment"]

    # Free slots of removed displays
    for slot, did in list(assignment.items()):
        if did not in current_displays:
            del assignment[slot]

    # Assign new displays to free slots
    assigned = set(assignment.values())
    free_slots = [s for s in range(1, MAX_DISPLAYS + 1) if s not in assignment]
    for did in sorted(current_displays - assigned):
        if not free_slots:
            break
        assignment[free_slots.pop(0)] = did


def refresh_display_mapping(state):
    """Refresh SketchyBar display info and update item properties."""
    state["arrangement_map"] = query_sketchybar_displays()
    assign_slots(state)
    update_display_properties(state)
    update_click_scripts(state)


def update_display_properties(state):
    """Update each slot's items to show on the correct SketchyBar display."""
    args = ["sketchybar"]
    for slot, did in state["slot_assignment"].items():
        arr_id = state["arrangement_map"].get(did)
        if arr_id is None:
            continue
        position = "e" if did == NOTCH_DISPLAY_ID else "center"
        for tag_num in range(1, NUM_TAGS + 1):
            item = f"space.d{slot}.{tag_num}"
            args.extend(["--set", item, f"display={arr_id}", f"position={position}"])
    if len(args) > 1:
        subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def update_click_scripts(state):
    """Set click_script on each item to target the correct yashiki display."""
    args = ["sketchybar"]
    for slot, did in state["slot_assignment"].items():
        for tag_num in range(1, NUM_TAGS + 1):
            bitmask = 1 << (tag_num - 1)
            item = f"space.d{slot}.{tag_num}"
            script = f"yashiki tag-view --output {did} {bitmask}"
            args.extend(["--set", item, f"click_script={script}"])
    if len(args) > 1:
        subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def update_all(state):
    """Re-render all items based on current state."""
    # Per-display occupied tags (keyed by display_id string)
    occupied_per_display = {}
    for winfo in state["windows"].values():
        did = winfo["output_id"]
        occupied_per_display[did] = occupied_per_display.get(did, 0) | winfo["tags"]

    active_slots = set(state["slot_assignment"].keys())

    args = ["sketchybar"]

    for slot in range(1, MAX_DISPLAYS + 1):
        bracket = f"workspaces.d{slot}"

        if slot not in active_slots:
            for tag_num in range(1, NUM_TAGS + 1):
                args.extend(["--set", f"space.d{slot}.{tag_num}", "drawing=off"])
            args.extend(["--set", bracket, "background.drawing=off"])
            continue

        did = state["slot_assignment"][slot]
        display_info = state["displays"].get(did, {})
        visible_tags = display_info.get("visible_tags", 0)
        display_occupied = occupied_per_display.get(did, 0)
        focused = did == state["focused_display"]

        # Update bracket
        bg_color = FOCUSED_BRACKET_BG if focused else UNFOCUSED_BRACKET_BG
        args.extend(
            ["--set", bracket, f"background.color={bg_color}", "background.drawing=on"]
        )

        for tag_num in range(1, NUM_TAGS + 1):
            bitmask = 1 << (tag_num - 1)
            item = f"space.d{slot}.{tag_num}"
            is_active = (visible_tags & bitmask) != 0
            is_occupied = (display_occupied & bitmask) != 0

            props = ["drawing=on"]

            if is_active:
                icon_color = FOCUSED_ACTIVE_ICON if focused else UNFOCUSED_ACTIVE_ICON
                bg = FOCUSED_ACTIVE_BG if focused else UNFOCUSED_ACTIVE_BG
                props.extend(
                    [
                        "background.drawing=on",
                        f"background.color={bg}",
                        "background.corner_radius=3",
                        "background.height=18",
                        f"icon.color={icon_color}",
                    ]
                )
            elif is_occupied:
                icon_color = (
                    FOCUSED_OCCUPIED_ICON if focused else UNFOCUSED_OCCUPIED_ICON
                )
                props.extend(["background.drawing=off", f"icon.color={icon_color}"])
            else:
                icon_color = FOCUSED_VACANT_ICON if focused else UNFOCUSED_VACANT_ICON
                props.extend(["background.drawing=off", f"icon.color={icon_color}"])

            args.extend(["--set", item] + props)

    subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def process_snapshot(event, state):
    """Initialize state from snapshot event."""
    state["displays"] = {
        str(d["id"]): {"visible_tags": d["visible_tags"]}
        for d in event["displays"]
    }
    state["windows"] = {
        str(w["id"]): {"tags": w["tags"], "output_id": str(w["output_id"])}
        for w in event["windows"]
    }
    state["focused_display"] = str(event["focused_display_id"])


def process_event(event, state):
    """Process an incremental event. Returns True if state changed."""
    t = event["type"]
    if t == "tags_changed":
        did = str(event["display_id"])
        if did in state["displays"]:
            state["displays"][did]["visible_tags"] = event["visible_tags"]
    elif t == "display_focused":
        state["focused_display"] = str(event["display_id"])
    elif t in ("window_created", "window_updated"):
        w = event["window"]
        state["windows"][str(w["id"])] = {
            "tags": w["tags"],
            "output_id": str(w["output_id"]),
        }
    elif t == "window_destroyed":
        state["windows"].pop(str(event["window_id"]), None)
    elif t in ("display_added", "display_updated"):
        d = event["display"]
        state["displays"][str(d["id"])] = {"visible_tags": d["visible_tags"]}
        time.sleep(0.5)
        refresh_display_mapping(state)
    elif t == "display_removed":
        state["displays"].pop(str(event["display_id"]), None)
        time.sleep(0.5)
        refresh_display_mapping(state)
    else:
        return False
    return True


def run():
    kill_old_instances()

    while True:
        try:
            proc = subprocess.Popen(
                [
                    "reap",
                    "--",
                    "yashiki",
                    "subscribe",
                    "--snapshot",
                    "--filter",
                    "tags,focus,window,display",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
            )
            state = {
                "displays": {},
                "windows": {},
                "focused_display": "0",
                "slot_assignment": {},
                "arrangement_map": {},
            }

            for line in proc.stdout:
                line = line.strip()
                if not line:
                    continue
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue

                if event["type"] == "snapshot":
                    process_snapshot(event, state)
                    refresh_display_mapping(state)
                else:
                    if not process_event(event, state):
                        continue

                update_all(state)

        except FileNotFoundError:
            print("yashiki_bridge: yashiki not found in PATH", file=sys.stderr)
        except Exception as e:
            print(f"yashiki_bridge: {e}", file=sys.stderr)

        time.sleep(2)


if __name__ == "__main__":
    run()
