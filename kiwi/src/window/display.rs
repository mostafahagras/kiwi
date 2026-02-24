use crate::ffi::*;
use core_graphics_types::geometry::CGRect;

pub fn get_main_display_bounds() -> CGRect {
    unsafe { CGDisplayBounds(CGMainDisplayID()) }
}
