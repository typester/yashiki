#!/bin/bash

# Event bridge between yashiki and SketchyBar.
#
# Subscribes to yashiki state events, tracks workspace state,
# and triggers SketchyBar updates via custom events.
# Runs as a long-lived background process, started from sketchybarrc.

if ! command -v jq &>/dev/null; then
  echo "yashiki_bridge: jq is required (brew install jq)" >&2
  exit 1
fi

if ! command -v yashiki &>/dev/null; then
  echo "yashiki_bridge: yashiki not found in PATH" >&2
  exit 1
fi

# State stored as JSON: {"displays": {id: visible_tags}, "windows": {id: tags}, "focused": id}
STATE='{"displays":{},"windows":{},"focused":0}'

compute_and_trigger() {
  local active occupied=0
  active=$(echo "$STATE" | jq -r '.displays[.focused | tostring] // 0')
  local t
  for t in $(echo "$STATE" | jq -r '.windows[]'); do
    occupied=$((occupied | t))
  done
  sketchybar --trigger yashiki_workspace_change \
    ACTIVE_TAGS="$active" \
    OCCUPIED_TAGS="$occupied"
}

process_snapshot() {
  local line="$1"
  STATE=$(echo "$line" | jq '{
    displays: (.displays | map({(.id | tostring): .visible_tags}) | add // {}),
    windows: (.windows | map({(.id | tostring): .tags}) | add // {}),
    focused: (.focused_display_id | tostring)
  }')
  compute_and_trigger
}

process_event() {
  local line="$1"
  local event_type
  event_type=$(echo "$line" | jq -r '.type')

  case "$event_type" in
    tags_changed)
      local did vtags
      did=$(echo "$line" | jq -r '.display_id')
      vtags=$(echo "$line" | jq -r '.visible_tags')
      STATE=$(echo "$STATE" | jq --arg did "$did" --argjson vtags "$vtags" \
        '.displays[$did] = $vtags')
      compute_and_trigger
      ;;
    display_focused)
      local did
      did=$(echo "$line" | jq -r '.display_id')
      STATE=$(echo "$STATE" | jq --arg did "$did" '.focused = $did')
      compute_and_trigger
      ;;
    window_created|window_updated)
      local wid wtags
      wid=$(echo "$line" | jq -r '.window.id')
      wtags=$(echo "$line" | jq -r '.window.tags')
      STATE=$(echo "$STATE" | jq --arg wid "$wid" --argjson wtags "$wtags" \
        '.windows[$wid] = $wtags')
      compute_and_trigger
      ;;
    window_destroyed)
      local wid
      wid=$(echo "$line" | jq -r '.window_id')
      STATE=$(echo "$STATE" | jq --arg wid "$wid" 'del(.windows[$wid])')
      compute_and_trigger
      ;;
  esac
}

# Main loop: subscribe to yashiki, retry on disconnect
while true; do
  yashiki subscribe --snapshot --filter tags,focus,window 2>/dev/null | while IFS= read -r line; do
    [ -z "$line" ] && continue

    event_type=$(echo "$line" | jq -r '.type')

    if [ "$event_type" = "snapshot" ]; then
      process_snapshot "$line"
    else
      process_event "$line"
    fi
  done

  sleep 2
done
