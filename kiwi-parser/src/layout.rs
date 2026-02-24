use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCreateInputSourceList(
        properties: *const std::ffi::c_void,
        includeAll: bool,
    ) -> CFArrayRef;
    fn TISGetInputSourceProperty(source: *const std::ffi::c_void, key: CFStringRef) -> CFStringRef;
    static kTISPropertyInputSourceID: CFStringRef;
}

pub fn list_available_layouts() -> Vec<String> {
    let mut layouts = Vec::new();
    unsafe {
        let source_list_ref = TISCreateInputSourceList(std::ptr::null(), false);
        if !source_list_ref.is_null() {
            let source_list = CFArray::<*const c_void>::wrap_under_get_rule(source_list_ref);
            for i in 0..source_list.len() {
                let source = *source_list.get(i).unwrap();
                let id_ref = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
                if !id_ref.is_null() {
                    let id_cfstring = CFString::wrap_under_get_rule(id_ref);
                    layouts.push(id_cfstring.to_string());
                }
            }
            CFRelease(source_list_ref as CFTypeRef);
        }
    }
    layouts
}

fn normalize_input(input: &str) -> String {
    let processed = input.replace("-", "–");
    if processed.contains('.') {
        processed.to_lowercase()
    } else {
        format!("com.apple.keylayout.{}", processed).to_lowercase()
    }
}

pub fn resolve_layout(input: &str) -> Option<String> {
    let available = list_available_layouts();
    let target = normalize_input(input);
    let apple_prefix = "com.apple.keylayout.";

    for id in available {
        let id_low = id.to_lowercase();
        if id_low == target {
            return Some(id);
        }
        if let Some(stripped) = id_low.strip_prefix(apple_prefix) {
            if stripped == target {
                return Some(id);
            }
        }
    }
    None
}

/// Suggests a layout while stripping the bulky Apple prefix for the user.
pub fn suggest_layout_fuzzy(input: &str) -> Option<String> {
    if resolve_layout(input).is_some() {
        return None;
    }
    let available = list_available_layouts();
    let target = normalize_input(input);
    let apple_prefix = "com.apple.keylayout.";

    let best_match = available.iter().min_by_key(|id| {
        let id_low = id.to_lowercase();
        let full_dist = strsim::levenshtein(&id_low, &target);
        let stripped_dist = id_low
            .strip_prefix(apple_prefix)
            .map(|s| strsim::levenshtein(s, &target))
            .unwrap_or(full_dist);

        full_dist.min(stripped_dist)
    })?;

    let id_low = best_match.to_lowercase();
    let dist = strsim::levenshtein(&id_low, &target);

    if dist > 5 {
        return None;
    }

    Some(
        best_match
            .strip_prefix(apple_prefix)
            .unwrap_or(best_match)
            .to_string(),
    )
}
