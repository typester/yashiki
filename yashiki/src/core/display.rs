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
    pub window_order: Vec<WindowId>,
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
            window_order: Vec::new(),
            current_layout: None,
            previous_layout: None,
        }
    }
}
