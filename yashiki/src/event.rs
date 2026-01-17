use core_graphics::geometry::{CGPoint, CGSize};

#[derive(Debug)]
pub enum Event {
    WindowCreated { pid: i32 },
    WindowDestroyed { pid: i32 },
    WindowFocused { pid: i32 },
}

#[derive(Debug)]
pub enum Command {
    MoveWindow { pid: i32, position: CGPoint },
    ResizeWindow { pid: i32, size: CGSize },
    MoveFocusedWindow { position: CGPoint },
    Quit,
}
