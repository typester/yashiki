use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::macos::DisplayId;

use super::{Rect, Tag, WindowId};

#[derive(Debug, Clone)]
pub struct Display {
    pub id: DisplayId,
    pub name: String,
    pub frame: Rect,
    pub is_main: bool,
    pub visible_tags: Tag,
    pub previous_visible_tags: Tag,
    pub tag_orders: BTreeMap<u8, Vec<WindowId>>,
    /// Per-tag last-focused window with timestamp. Used to restore focus to the
    /// most recently-used window on a tag when switching back. For multi-bit
    /// visible_tags, the entry with the latest timestamp wins.
    pub last_focused_per_tag: HashMap<u8, (WindowId, Instant)>,
    pub current_layout: Option<String>,
    pub previous_layout: Option<String>,
}

impl Display {
    pub fn new(id: DisplayId, name: String, frame: Rect, is_main: bool) -> Self {
        Self {
            id,
            name,
            frame,
            is_main,
            visible_tags: Tag::new(1),
            previous_visible_tags: Tag::new(1),
            tag_orders: BTreeMap::new(),
            last_focused_per_tag: HashMap::new(),
            current_layout: None,
            previous_layout: None,
        }
    }
}
