use super::Tag;
use crate::macos::{Bounds, DisplayId, WindowInfo};
use yashiki_ipc::ButtonInfo;

pub type WindowId = u32;

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub pid: i32,
    pub display_id: DisplayId,
    pub tags: Tag,
    pub title: String,
    pub app_name: String,
    pub app_id: Option<String>,
    pub ax_id: Option<String>,
    pub subrole: Option<String>,
    pub window_level: i32,
    pub close_button: ButtonInfo,
    pub fullscreen_button: ButtonInfo,
    pub minimize_button: ButtonInfo,
    pub zoom_button: ButtonInfo,
    pub frame: Rect,
    pub saved_frame: Option<Rect>,
    pub is_floating: bool,
    pub is_fullscreen: bool,
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
            app_id: info.bundle_id.clone(),
            ax_id: None,
            subrole: None,
            window_level: info.layer,
            close_button: ButtonInfo::default(),
            fullscreen_button: ButtonInfo::default(),
            minimize_button: ButtonInfo::default(),
            zoom_button: ButtonInfo::default(),
            frame: Rect::from_bounds(&info.bounds),
            saved_frame: None,
            is_floating: false,
            is_fullscreen: false,
        }
    }

    pub fn is_tiled(&self) -> bool {
        !self.is_floating && !self.is_fullscreen
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

    /// Get extended window attributes for rule matching
    pub fn extended_attributes(&self) -> yashiki_ipc::ExtendedWindowAttributes {
        yashiki_ipc::ExtendedWindowAttributes {
            ax_id: self.ax_id.clone(),
            subrole: self.subrole.clone(),
            window_level: self.window_level,
            close_button: self.close_button.clone(),
            fullscreen_button: self.fullscreen_button.clone(),
            minimize_button: self.minimize_button.clone(),
            zoom_button: self.zoom_button.clone(),
        }
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
