use core_graphics::geometry::{CGPoint, CGSize};

#[derive(Debug, Clone)]
pub enum Event {
    WindowCreated { pid: i32 },
    WindowDestroyed { pid: i32 },
    FocusedWindowChanged { pid: i32 },
    WindowMoved { pid: i32 },
    WindowResized { pid: i32 },
    WindowMiniaturized { pid: i32 },
    WindowDeminiaturized { pid: i32 },
    ApplicationActivated { pid: i32 },
    ApplicationDeactivated { pid: i32 },
    ApplicationHidden { pid: i32 },
    ApplicationShown { pid: i32 },
}

#[derive(Debug)]
pub enum Command {
    MoveWindow { pid: i32, position: CGPoint },
    ResizeWindow { pid: i32, size: CGSize },
    MoveFocusedWindow { position: CGPoint },
    Quit,
}
