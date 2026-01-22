use serde::{Deserialize, Serialize};

use crate::{OutputInfo, WindowInfo};

/// Event filter for subscribing to specific event types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventFilter {
    /// Subscribe to window events (created, destroyed, updated)
    #[serde(default)]
    pub window: bool,
    /// Subscribe to focus events (window focused, display focused)
    #[serde(default)]
    pub focus: bool,
    /// Subscribe to display events (added, removed, updated)
    #[serde(default)]
    pub display: bool,
    /// Subscribe to tag change events
    #[serde(default)]
    pub tags: bool,
    /// Subscribe to layout change events
    #[serde(default)]
    pub layout: bool,
}

impl EventFilter {
    /// Create a filter that subscribes to all events
    pub fn all() -> Self {
        Self {
            window: true,
            focus: true,
            display: true,
            tags: true,
            layout: true,
        }
    }

    /// Check if the filter matches a given event
    pub fn matches(&self, event: &StateEvent) -> bool {
        match event {
            StateEvent::WindowCreated { .. }
            | StateEvent::WindowDestroyed { .. }
            | StateEvent::WindowUpdated { .. } => self.window,
            StateEvent::WindowFocused { .. } | StateEvent::DisplayFocused { .. } => self.focus,
            StateEvent::DisplayAdded { .. }
            | StateEvent::DisplayRemoved { .. }
            | StateEvent::DisplayUpdated { .. } => self.display,
            StateEvent::TagsChanged { .. } => self.tags,
            StateEvent::LayoutChanged { .. } => self.layout,
            StateEvent::Snapshot { .. } => true, // Snapshots always pass filter
        }
    }

    /// Check if any filter is set
    pub fn any(&self) -> bool {
        self.window || self.focus || self.display || self.tags || self.layout
    }
}

/// Request to subscribe to state events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscribeRequest {
    /// Whether to send a snapshot on connection
    #[serde(default)]
    pub snapshot: bool,
    /// Event filter (if not set or all false, subscribes to all events)
    #[serde(default)]
    pub filter: EventFilter,
}

impl SubscribeRequest {
    /// Create a subscribe request with snapshot enabled
    pub fn with_snapshot() -> Self {
        Self {
            snapshot: true,
            filter: EventFilter::default(),
        }
    }

    /// Get the effective filter (all if none specified)
    pub fn effective_filter(&self) -> EventFilter {
        if self.filter.any() {
            self.filter.clone()
        } else {
            EventFilter::all()
        }
    }
}

/// State change events sent to subscribers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateEvent {
    // Window events
    WindowCreated {
        window: WindowInfo,
    },
    WindowDestroyed {
        window_id: u32,
    },
    WindowUpdated {
        window: WindowInfo,
    },

    // Focus events
    WindowFocused {
        window_id: Option<u32>,
    },
    DisplayFocused {
        display_id: u32,
    },

    // Display events
    DisplayAdded {
        display: OutputInfo,
    },
    DisplayRemoved {
        display_id: u32,
    },
    DisplayUpdated {
        display: OutputInfo,
    },

    // Tag events
    TagsChanged {
        display_id: u32,
        visible_tags: u32,
        previous_tags: u32,
    },

    // Layout events
    LayoutChanged {
        display_id: u32,
        layout: String,
    },

    // Full snapshot
    Snapshot {
        windows: Vec<WindowInfo>,
        displays: Vec<OutputInfo>,
        focused_window_id: Option<u32>,
        focused_display_id: u32,
        default_layout: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_all() {
        let filter = EventFilter::all();
        assert!(filter.window);
        assert!(filter.focus);
        assert!(filter.display);
        assert!(filter.tags);
        assert!(filter.layout);
    }

    #[test]
    fn test_event_filter_matches() {
        let window_filter = EventFilter {
            window: true,
            ..Default::default()
        };

        assert!(window_filter.matches(&StateEvent::WindowCreated {
            window: WindowInfo {
                id: 1,
                pid: 100,
                title: "Test".to_string(),
                app_name: "App".to_string(),
                app_id: None,
                tags: 1,
                x: 0,
                y: 0,
                width: 100,
                height: 100,
                is_focused: false,
                is_floating: false,
                is_fullscreen: false,
                output_id: 1,
                status: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            }
        }));
        assert!(window_filter.matches(&StateEvent::WindowDestroyed { window_id: 1 }));
        assert!(!window_filter.matches(&StateEvent::WindowFocused { window_id: Some(1) }));
        assert!(!window_filter.matches(&StateEvent::TagsChanged {
            display_id: 1,
            visible_tags: 1,
            previous_tags: 2,
        }));
    }

    #[test]
    fn test_subscribe_request_effective_filter() {
        // Default should return all
        let req = SubscribeRequest::default();
        let effective = req.effective_filter();
        assert!(effective.window);
        assert!(effective.focus);

        // Specific filter should be preserved
        let req = SubscribeRequest {
            snapshot: false,
            filter: EventFilter {
                focus: true,
                ..Default::default()
            },
        };
        let effective = req.effective_filter();
        assert!(!effective.window);
        assert!(effective.focus);
    }

    #[test]
    fn test_state_event_serialization() {
        let event = StateEvent::WindowFocused {
            window_id: Some(123),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"window_focused\""));
        assert!(json.contains("\"window_id\":123"));

        let deserialized: StateEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            StateEvent::WindowFocused { window_id } => assert_eq!(window_id, Some(123)),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_tags_changed_serialization() {
        let event = StateEvent::TagsChanged {
            display_id: 1,
            visible_tags: 0b0010,
            previous_tags: 0b0001,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tags_changed\""));
        assert!(json.contains("\"display_id\":1"));
        assert!(json.contains("\"visible_tags\":2"));
        assert!(json.contains("\"previous_tags\":1"));

        let deserialized: StateEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            StateEvent::TagsChanged {
                display_id,
                visible_tags,
                previous_tags,
            } => {
                assert_eq!(display_id, 1);
                assert_eq!(visible_tags, 0b0010);
                assert_eq!(previous_tags, 0b0001);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_snapshot_serialization() {
        let event = StateEvent::Snapshot {
            windows: vec![],
            displays: vec![],
            focused_window_id: Some(42),
            focused_display_id: 1,
            default_layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"snapshot\""));
        assert!(json.contains("\"focused_window_id\":42"));
        assert!(json.contains("\"default_layout\":\"tatami\""));
    }

    #[test]
    fn test_subscribe_request_serialization() {
        let req = SubscribeRequest {
            snapshot: true,
            filter: EventFilter {
                focus: true,
                tags: true,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"snapshot\":true"));
        assert!(json.contains("\"focus\":true"));
        assert!(json.contains("\"tags\":true"));

        let deserialized: SubscribeRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.snapshot);
        assert!(deserialized.filter.focus);
        assert!(deserialized.filter.tags);
        assert!(!deserialized.filter.window);
    }

    #[test]
    fn test_layout_changed_serialization() {
        let event = StateEvent::LayoutChanged {
            display_id: 1,
            layout: "byobu".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"layout_changed\""));
        assert!(json.contains("\"layout\":\"byobu\""));

        let deserialized: StateEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            StateEvent::LayoutChanged { display_id, layout } => {
                assert_eq!(display_id, 1);
                assert_eq!(layout, "byobu");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_display_events_serialization() {
        let added = StateEvent::DisplayAdded {
            display: OutputInfo {
                id: 1,
                name: "Main".to_string(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                is_main: true,
                visible_tags: 1,
                is_focused: true,
            },
        };
        let json = serde_json::to_string(&added).unwrap();
        assert!(json.contains("\"type\":\"display_added\""));

        let removed = StateEvent::DisplayRemoved { display_id: 2 };
        let json = serde_json::to_string(&removed).unwrap();
        assert!(json.contains("\"type\":\"display_removed\""));
        assert!(json.contains("\"display_id\":2"));
    }
}
