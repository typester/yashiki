use super::{Rect, Tag};
use crate::macos::DisplayId;

#[derive(Debug, Clone)]
pub struct Display {
    pub id: DisplayId,
    pub frame: Rect,
    pub visible_tags: Tag,
}

impl Display {
    pub fn new(id: DisplayId, frame: Rect) -> Self {
        Self {
            id,
            frame,
            visible_tags: Tag::new(1),
        }
    }
}
