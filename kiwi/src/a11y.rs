#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

pub fn is_process_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
