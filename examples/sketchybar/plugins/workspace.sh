#!/bin/bash

# SketchyBar plugin for yashiki tag indicators.
#
# Called when the yashiki_workspace_change event fires.
# Arguments: $1 = tag number (1-10)
# Environment (from sketchybar --trigger):
#   ACTIVE_TAGS   - bitmask of visible tags on the focused display
#   OCCUPIED_TAGS - bitmask of tags that have at least one window

TAG_NUM="${1:-1}"
BITMASK=$((1 << (TAG_NUM - 1)))

IS_ACTIVE=$(( (ACTIVE_TAGS & BITMASK) != 0 ))
IS_OCCUPIED=$(( (OCCUPIED_TAGS & BITMASK) != 0 ))

if [[ "$IS_ACTIVE" -eq 1 ]]; then
  sketchybar --set "$NAME" \
    background.drawing=on \
    icon.highlight=on
elif [[ "$IS_OCCUPIED" -eq 1 ]]; then
  sketchybar --set "$NAME" \
    background.drawing=off \
    icon.highlight=off \
    icon.color=0xffffffff
else
  sketchybar --set "$NAME" \
    background.drawing=off \
    icon.highlight=off \
    icon.color=0xff555555
fi
