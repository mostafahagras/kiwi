use crate::ffi::{
    AXError, AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementPerformAction,
    AXUIElementSetAttributeValue,
};
use core_foundation::base::CFTypeRef;
use core_foundation::string::CFStringRef;
use kiwi_parser::MenubarAction;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSArray, NSString};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *mut c_void);
    fn CFBooleanGetValue(boolean: CFBooleanRef) -> bool;
}

type AXUIElementRef = CFTypeRef;
type CFBooleanRef = *const c_void;

const K_AX_ERROR_SUCCESS: AXError = AXError::Success;

#[derive(Debug)]
struct AxElement(AXUIElementRef);

impl AxElement {
    unsafe fn new(ptr: AXUIElementRef) -> Self {
        Self(ptr)
    }

    unsafe fn from_borrowed(ptr: AXUIElementRef) -> Self {
        unsafe {
            CFRetain(ptr);
        }
        Self(ptr)
    }

    fn as_ptr(&self) -> AXUIElementRef {
        self.0
    }
}

impl Clone for AxElement {
    fn clone(&self) -> Self {
        unsafe {
            CFRetain(self.0);
        }
        Self(self.0)
    }
}

impl Drop for AxElement {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0 as *mut c_void);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuPathCandidate {
    path: Vec<String>,
    has_submenu: bool,
}

#[derive(Debug)]
struct MenuEntry {
    candidate: MenuPathCandidate,
}

static MENU_PATH_CACHE: OnceLock<Mutex<HashMap<i32, Vec<MenuPathCandidate>>>> = OnceLock::new();

fn menu_path_cache() -> &'static Mutex<HashMap<i32, Vec<MenuPathCandidate>>> {
    MENU_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn display_path(path: &[String]) -> String {
    path.join(" > ")
}

fn normalize_component(component: &str) -> String {
    component.trim().to_lowercase()
}

fn normalize_path(path: &[String]) -> Vec<String> {
    path.iter()
        .map(|component| normalize_component(component))
        .collect()
}

fn path_matches_suffix(candidate: &[String], target: &[String]) -> bool {
    if target.is_empty() || candidate.len() < target.len() {
        return false;
    }

    let candidate_suffix = &candidate[candidate.len() - target.len()..];
    candidate_suffix
        .iter()
        .zip(target)
        .all(|(left, right)| normalize_component(left) == normalize_component(right))
}

fn match_menu_candidates(
    candidates: &[MenuPathCandidate],
    target: &[String],
    leaf_only: bool,
) -> Result<usize, String> {
    let normalized_target = normalize_path(target);
    if normalized_target.is_empty() {
        return Err("menu path cannot be empty".into());
    }

    let hits: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| !leaf_only || !candidate.has_submenu)
        .filter(|(_, candidate)| path_matches_suffix(&candidate.path, &normalized_target))
        .map(|(idx, _)| idx)
        .collect();

    match hits.as_slice() {
        [] => Err(format!("no menu item matches '{}'", display_path(target))),
        [idx] => Ok(*idx),
        _ => {
            let mut paths: Vec<String> = hits
                .iter()
                .map(|idx| display_path(&candidates[*idx].path))
                .collect();
            paths.sort();
            paths.dedup();
            Err(format!(
                "menu path '{}' is ambiguous; matches: {}",
                display_path(target),
                paths.join(", ")
            ))
        }
    }
}

unsafe fn ax_get_string(element: AXUIElementRef, attribute: &NSString) -> Option<String> {
    let attr_cf = attribute as *const NSString as CFStringRef;
    let mut val: CFTypeRef = std::ptr::null();
    if unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut val) } == K_AX_ERROR_SUCCESS
        && !val.is_null()
    {
        let ns_str: Retained<NSString> = unsafe { Retained::from_raw(val as *mut NSString) }?;
        Some(ns_str.to_string())
    } else {
        None
    }
}

fn ax_role(element: &AxElement) -> Option<String> {
    unsafe { ax_get_string(element.as_ptr(), &NSString::from_str("AXRole")) }
}

fn ax_title(element: &AxElement) -> Option<String> {
    unsafe { ax_get_string(element.as_ptr(), &NSString::from_str("AXTitle")) }
}

fn ax_parent(element: &AxElement) -> Option<AxElement> {
    let parent_attr = &NSString::from_str("AXParent");
    let parent_attr_cf = parent_attr.as_ref() as *const NSString as CFStringRef;
    let mut val: CFTypeRef = std::ptr::null();
    unsafe {
        if AXUIElementCopyAttributeValue(element.as_ptr(), parent_attr_cf, &mut val)
            == K_AX_ERROR_SUCCESS
            && !val.is_null()
        {
            Some(AxElement::new(val as AXUIElementRef))
        } else {
            None
        }
    }
}

fn ax_children(element: &AxElement) -> Vec<AxElement> {
    let children_attr = &NSString::from_str("AXChildren");
    let children_attr_cf = children_attr.as_ref() as *const NSString as CFStringRef;
    let mut children_val: CFTypeRef = std::ptr::null();
    unsafe {
        if AXUIElementCopyAttributeValue(element.as_ptr(), children_attr_cf, &mut children_val)
            == K_AX_ERROR_SUCCESS
            && !children_val.is_null()
        {
            let arr_retained =
                Retained::from_raw(children_val as *mut NSArray<objc2::runtime::AnyObject>);
            let mut list = Vec::new();
            if let Some(arr) = arr_retained {
                for child in &*arr {
                    let child_ref = &*child as *const AnyObject as AXUIElementRef;
                    list.push(AxElement::from_borrowed(child_ref));
                }
            }
            list
        } else {
            Vec::new()
        }
    }
}

fn ax_is_enabled(element: &AxElement) -> bool {
    let enabled_attr = &NSString::from_str("AXEnabled");
    let enabled_attr_cf = enabled_attr.as_ref() as *const NSString as CFStringRef;
    let mut enabled_val: CFTypeRef = std::ptr::null();
    unsafe {
        if AXUIElementCopyAttributeValue(element.as_ptr(), enabled_attr_cf, &mut enabled_val)
            == K_AX_ERROR_SUCCESS
            && !enabled_val.is_null()
        {
            let b = CFBooleanGetValue(enabled_val as CFBooleanRef);
            CFRelease(enabled_val as *mut c_void);
            b
        } else {
            true
        }
    }
}

fn menu_bar(app: &NSRunningApplication) -> Option<AxElement> {
    let pid = app.processIdentifier();
    unsafe {
        let ax_app = AXUIElementCreateApplication(pid);
        if ax_app.is_null() {
            return None;
        }
        let ax_app = AxElement::new(ax_app);

        let menu_bar_attr = &NSString::from_str("AXMenuBar");
        let menu_bar_attr_cf = menu_bar_attr.as_ref() as *const NSString as CFStringRef;
        let mut menu_bar_val: CFTypeRef = std::ptr::null();
        let err =
            AXUIElementCopyAttributeValue(ax_app.as_ptr(), menu_bar_attr_cf, &mut menu_bar_val);
        if err == K_AX_ERROR_SUCCESS && !menu_bar_val.is_null() {
            Some(AxElement::new(menu_bar_val as AXUIElementRef))
        } else {
            None
        }
    }
}

fn collect_menu_entries(
    element: &AxElement,
    breadcrumb: &[String],
    depth: usize,
) -> Vec<MenuEntry> {
    if depth >= 8 {
        return Vec::new();
    }

    let role = ax_role(element).unwrap_or_default();
    if role == "AXSeparator" {
        return Vec::new();
    }

    let title = ax_title(element).unwrap_or_default();
    let mut crumb = breadcrumb.to_vec();
    if !title.is_empty() {
        crumb.push(title.clone());
    }

    let children = ax_children(element);
    let has_submenu = children
        .iter()
        .any(|child| ax_role(child).as_deref() == Some("AXMenu"));
    let mut results = Vec::new();
    if !title.is_empty() {
        results.push(MenuEntry {
            candidate: MenuPathCandidate {
                path: crumb.clone(),
                has_submenu,
            },
        });
    }

    for child in &children {
        results.extend(collect_menu_entries(child, &crumb, depth + 1));
    }

    results
}

fn collect_menu_candidates(
    element: &AxElement,
    breadcrumb: &[String],
    depth: usize,
) -> Vec<MenuPathCandidate> {
    if depth >= 8 {
        return Vec::new();
    }

    let role = ax_role(element).unwrap_or_default();
    if role == "AXSeparator" {
        return Vec::new();
    }

    let title = ax_title(element).unwrap_or_default();
    let mut crumb = breadcrumb.to_vec();
    if !title.is_empty() {
        crumb.push(title);
    }

    let children = ax_children(element);
    let has_submenu = children
        .iter()
        .any(|child| ax_role(child).as_deref() == Some("AXMenu"));
    let mut results = Vec::new();
    if !crumb.is_empty() && crumb.len() > breadcrumb.len() {
        results.push(MenuPathCandidate {
            path: crumb.clone(),
            has_submenu,
        });
    }
    for child in &children {
        results.extend(collect_menu_candidates(child, &crumb, depth + 1));
    }
    results
}

fn find_menu_component(element: &AxElement, target_name: &str) -> Option<AxElement> {
    let title = ax_title(element).unwrap_or_default();

    if !title.is_empty() && normalize_component(&title) == normalize_component(target_name) {
        return Some(element.clone());
    }

    let children = ax_children(element);
    for child in &children {
        let child_role = ax_role(child).unwrap_or_default();
        let child_title = ax_title(child).unwrap_or_default();

        if !child_title.is_empty()
            && normalize_component(&child_title) == normalize_component(target_name)
        {
            return Some(child.clone());
        }

        if (child_role == "AXMenu" || child_title.is_empty())
            && let Some(found) = find_menu_component(child, target_name)
        {
            return Some(found);
        }
    }

    None
}

fn press(element: &AxElement) -> Result<(), String> {
    let press_action = &NSString::from_str("AXPress");
    let press_action_cf = press_action.as_ref() as *const NSString as CFStringRef;
    let result = unsafe { AXUIElementPerformAction(element.as_ptr(), press_action_cf) };
    if result == K_AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("AXPress failed with code {:?}", result))
    }
}

fn click_path(app_name: &str, menu_bar: &AxElement, path: &[String]) -> Result<(), String> {
    if path.is_empty() {
        return Err("menu path cannot be empty".into());
    }

    let mut current = menu_bar.clone();
    for (index, component) in path.iter().enumerate() {
        let found = find_menu_component(&current, component).ok_or_else(|| {
            format!(
                "menu component '{}' not found while clicking '{}' in '{app_name}'",
                component,
                display_path(path)
            )
        })?;
        let is_last = index + 1 == path.len();
        if is_last {
            if !ax_is_enabled(&found) {
                return Err(format!(
                    "menu item '{}' is disabled in '{app_name}'",
                    display_path(path)
                ));
            }
            return press(&found).map_err(|e| {
                format!(
                    "failed to click menu item '{}' in '{app_name}': {e}",
                    display_path(path)
                )
            });
        }

        // AX exposes submenu descendants without opening their parent. Keep walking
        // the accessibility tree and press only the final item so `menubar:click`
        // does not visibly open the menu hierarchy.
        current = found;
    }
    unreachable!()
}

fn focus_app(app: &NSRunningApplication) {
    let options = NSApplicationActivationOptions::ActivateAllWindows;
    let _ = app.activateWithOptions(options);
    std::thread::sleep(Duration::from_millis(300));
}

fn resolve_target_app(
    app_name: Option<&str>,
) -> Result<(Retained<NSRunningApplication>, String), String> {
    let workspace = NSWorkspace::sharedWorkspace();
    match app_name {
        Some(target_name) => {
            let target_name = target_name.trim();
            if target_name.is_empty() {
                return Err("app name cannot be empty".into());
            }

            let running_apps = workspace.runningApplications();
            for app in &running_apps {
                if let Some(localized_name) = app.localizedName()
                    && localized_name.to_string().eq_ignore_ascii_case(target_name)
                {
                    return Ok((app.clone(), localized_name.to_string()));
                }
            }

            Err(format!("application '{target_name}' is not running"))
        }
        None => match workspace.frontmostApplication() {
            Some(app) => {
                let name = app
                    .localizedName()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                Ok((app, name))
            }
            None => Err("could not determine the frontmost application".into()),
        },
    }
}

fn resolve_menu_entries(app: &NSRunningApplication) -> Result<(AxElement, Vec<MenuEntry>), String> {
    let app_name = app
        .localizedName()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let mb = menu_bar(app).ok_or_else(|| format!("could not read menu bar for '{app_name}'"))?;
    let entries = collect_menu_entries(&mb, &[], 0);
    Ok((mb, entries))
}

fn menu_paths(entries: &[MenuEntry]) -> Vec<MenuPathCandidate> {
    entries
        .iter()
        .map(|entry| entry.candidate.clone())
        .collect()
}

fn highlight_leaf(found: &AxElement) -> Result<(), String> {
    let Some(parent) = ax_parent(found) else {
        return Ok(());
    };

    if ax_role(&parent).as_deref() != Some("AXMenu") {
        return Ok(());
    }

    let retained_found = unsafe { Retained::retain(found.as_ptr() as *mut AnyObject) }
        .ok_or_else(|| "failed to retain menu item for highlight".to_string())?;
    let arr = NSArray::arrayWithObject(&*retained_found);
    let arr_cf = &*arr as *const _ as CFTypeRef;
    let sel_attr = &NSString::from_str("AXSelectedChildren");
    let sel_attr_cf = sel_attr.as_ref() as *const NSString as CFStringRef;
    let err = unsafe { AXUIElementSetAttributeValue(parent.as_ptr(), sel_attr_cf, arr_cf) };
    if err == K_AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("AXSelectedChildren failed with code {:?}", err))
    }
}

fn show_entry(
    app_name: &str,
    menu_bar: &AxElement,
    entries: &[MenuEntry],
    item: &[String],
) -> Result<(), String> {
    let candidates = menu_paths(entries);
    let idx = match_menu_candidates(&candidates, item, false)
        .map_err(|e| format!("{e} in '{app_name}'"))?;
    let matched = &entries[idx];

    let app_display = app_name.to_string();
    let components = matched.candidate.path.clone();
    if components.is_empty() {
        return Err("menu path cannot be empty".into());
    }

    let mut current_element = menu_bar.clone();
    for (i, comp) in components.iter().enumerate() {
        let found = find_menu_component(&current_element, comp).ok_or_else(|| {
            format!(
                "menu component '{}' not found while showing '{}' in '{app_display}'",
                comp,
                display_path(&matched.candidate.path)
            )
        })?;

        let is_last = i + 1 == components.len();
        if !is_last {
            press(&found).map_err(|e| {
                format!(
                    "failed to open menu component '{}' in '{app_display}': {e}",
                    comp
                )
            })?;
            std::thread::sleep(Duration::from_millis(200));
            current_element = found;
        } else {
            let has_submenu = ax_children(&found)
                .iter()
                .any(|child| ax_role(child).as_deref() == Some("AXMenu"));
            if has_submenu {
                press(&found).map_err(|e| {
                    format!(
                        "failed to open submenu '{}' in '{app_display}': {e}",
                        display_path(&matched.candidate.path)
                    )
                })?;
            } else {
                let _ = highlight_leaf(&found);
            }
        }
    }

    Ok(())
}

pub fn click(action: &MenubarAction) -> Result<(), String> {
    let (app, app_name) = resolve_target_app(action.app.as_deref())?;
    // focus_app(&app);
    let menu_bar =
        menu_bar(&app).ok_or_else(|| format!("could not read menu bar for '{app_name}'"))?;

    // A complete path can be followed directly without enumerating the menu tree.
    // Besides being the common configured form, this also keeps enabled state fresh.
    if action.item.len() > 1 {
        match click_path(&app_name, &menu_bar, &action.item) {
            Ok(()) => return Ok(()),
            Err(error) if error.contains("not found while clicking") => {}
            Err(error) => return Err(error),
        }
    }

    let pid = app.processIdentifier();
    for refresh in [false, true] {
        let candidates = if refresh {
            let paths = collect_menu_candidates(&menu_bar, &[], 0);
            menu_path_cache().lock().unwrap().insert(pid, paths.clone());
            paths
        } else {
            menu_path_cache()
                .lock()
                .unwrap()
                .get(&pid)
                .cloned()
                .unwrap_or_default()
        };

        if candidates.is_empty() {
            continue;
        }
        if let Ok(index) = match_menu_candidates(&candidates, &action.item, true)
            && click_path(&app_name, &menu_bar, &candidates[index].path).is_ok()
        {
            return Ok(());
        }
    }

    let candidates = menu_path_cache()
        .lock()
        .unwrap()
        .get(&pid)
        .cloned()
        .unwrap_or_default();
    let index = match_menu_candidates(&candidates, &action.item, true)
        .map_err(|e| format!("{e} in '{app_name}'"))?;
    click_path(&app_name, &menu_bar, &candidates[index].path)
}

pub fn show(action: &MenubarAction) -> Result<(), String> {
    let (app, app_name) = resolve_target_app(action.app.as_deref())?;
    focus_app(&app);
    let (menu_bar, entries) = resolve_menu_entries(&app)?;
    show_entry(&app_name, &menu_bar, &entries, &action.item)
}

#[cfg(test)]
mod tests {
    use super::{MenuPathCandidate, match_menu_candidates};

    fn candidate(path: &[&str], has_submenu: bool) -> MenuPathCandidate {
        MenuPathCandidate {
            path: path.iter().map(|part| part.to_string()).collect(),
            has_submenu,
        }
    }

    #[test]
    fn exact_path_match_wins() {
        let candidates = vec![
            candidate(&["File"], true),
            candidate(&["File", "New Tab"], false),
            candidate(&["Edit", "Copy"], false),
        ];

        let idx = match_menu_candidates(&candidates, &["File".into(), "New Tab".into()], true)
            .expect("should match");
        assert_eq!(idx, 1);
    }

    #[test]
    fn unique_leaf_suffix_matches() {
        let candidates = vec![
            candidate(&["File", "New Tab"], false),
            candidate(&["Edit", "Copy"], false),
        ];

        let idx = match_menu_candidates(&candidates, &["Copy".into()], true).expect("should match");
        assert_eq!(idx, 1);
    }

    #[test]
    fn ambiguous_suffix_is_rejected() {
        let candidates = vec![
            candidate(&["File", "Copy"], false),
            candidate(&["Edit", "Copy"], false),
        ];

        let err = match_menu_candidates(&candidates, &["Copy".into()], true)
            .expect_err("should be ambiguous");
        assert!(err.contains("ambiguous"));
        assert!(err.contains("File > Copy"));
        assert!(err.contains("Edit > Copy"));
    }
}
