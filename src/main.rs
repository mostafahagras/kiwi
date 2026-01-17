mod a11y;
mod window;
use std::error::Error;

use a11y::is_process_trusted;

fn main() -> Result<(), Box<dyn Error>> {
    if !is_process_trusted() {
        return Err("Accessibility permissions NOT granted.\nPlease authorize the app in System Settings -> Privacy & Security -> Accessibility.".into());
    }

    let windows = window::get_windows();
    println!("Found {} windows:", windows.len());
    for win in windows.iter().take(5) {
        println!("{:#?}", win);
    }

    if let Some(focused) = window::get_focused_window() {
        println!("\nFocused Window:\n{:#?}", focused);
        
        println!("\nMoving focused window to (0, 0) and resizing to (855, 1107)...");
        if window::set_focused_window_bounds(0.0, 0.0, 855.0, 1107.0) {
            println!("Success!");
        } else {
            println!("Failed to move/resize window.");
        }
    } else {
        println!("\nNo focused window found.");
    }

    Ok(())
}
