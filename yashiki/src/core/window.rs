use super::Tag;
use crate::macos::{Bounds, DisplayId, WindowInfo};

pub type WindowId = u32;

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub pid: i32,
    pub display_id: DisplayId,
    pub tags: Tag,
    pub title: String,
    pub app_name: String,
    pub frame: Rect,
    pub saved_frame: Option<Rect>,
    pub is_floating: bool,
}

impl Window {
    pub fn from_window_info(info: &WindowInfo, default_tag: Tag, display_id: DisplayId) -> Self {
        Self {
            id: info.window_id,
            pid: info.pid,
            display_id,
            tags: default_tag,
            title: info.name.clone().unwrap_or_default(),
            app_name: info.owner_name.clone(),
            frame: Rect::from_bounds(&info.bounds),
            saved_frame: None,
            is_floating: false,
        }
    }

    pub fn center(&self) -> (i32, i32) {
        (
            self.frame.x + self.frame.width as i32 / 2,
            self.frame.y + self.frame.height as i32 / 2,
        )
    }

    /// Check if window is hidden (has a saved frame from being moved offscreen)
    pub fn is_hidden(&self) -> bool {
        self.saved_frame.is_some()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn from_bounds(bounds: &Bounds) -> Self {
        Self {
            x: bounds.x as i32,
            y: bounds.y as i32,
            width: bounds.width as u32,
            height: bounds.height as u32,
        }
    }
}
