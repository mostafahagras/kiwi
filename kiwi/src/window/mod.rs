pub mod display;
pub mod focused;
pub mod list;
pub mod ops;

pub use display::get_main_display_bounds;
pub use focused::{
    get_focused_app, get_focused_window, get_focused_window_ref, get_frontmost_app_name,
};
pub use ops::set_focused_window_bounds;
