pub mod action;
mod app;
pub mod binding;
pub mod error;
pub mod layer;
mod utils;
pub use app::{AppEntry, AppSelector};

use crate::{
    config::{
        action::{Action, ParseScope, parse_action},
        app::parse_apps,
        binding::parse_keybinding,
        error::{ConfigError, MultiConfigError},
        layer::Layer,
        layer::parse_layers,
        utils::MODIFIER_SUGGESTIONS,
    },
    key::{KeyBinding, Modifiers},
    layout::{resolve_layout, suggest_layout_fuzzy},
};
use miette::{NamedSource, Report, SourceSpan};
use std::{collections::HashMap, path::PathBuf};
use toml_span::{parse, value::ValueInner};
use utils::suggest_best_match;

#[derive(Debug)]
pub struct Config {
    pub cwd: String,
    pub layout: Option<String>,
    pub menubar: MenubarConfig,
    pub global_binds: HashMap<KeyBinding, Action>,
    pub layers: HashMap<KeyBinding, Layer>,
    pub apps: Vec<AppEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct MenubarConfig {
    pub enabled: bool,
    pub max_len: usize,
}

pub struct ValidationContext<'a> {
    pub src: &'a NamedSource<String>,
    /// Maps sorted modifiers to their alias name (e.g., [Ctrl, Alt] -> "meh")
    pub modifier_map: &'a HashMap<Modifiers, (String, SourceSpan)>,
    /// Simple list of alias names for fuzzy matching (e.g., ["hyper", "meh"])
    pub modifier_names: Vec<String>,
    pub app_aliases: HashMap<String, String>,
    pub app_groups: HashMap<String, Vec<String>>,
}

pub fn parse_config(raw_toml: &str, path: PathBuf) -> Result<Config, Report> {
    let src = NamedSource::new(path.to_str().unwrap(), raw_toml.to_string());

    // 1. Handle TOML Syntax errors (like duplicate keys)
    let doc = match parse(raw_toml) {
        Ok(d) => d,
        Err(e) => {
            let span = SourceSpan::new(e.span.start.into(), e.span.end - e.span.start);
            return Err(Report::new(ConfigError::Syntax {
                src,
                span,
                message: format!("{:?}", e.kind),
            }));
        }
    };

    let root = doc
        .as_table()
        .ok_or_else(|| miette::miette!("Root is not a table"))?;
    let mut errors = Vec::new();

    // --- Working Directory Validation ---
    let mut cwd = "$KIWI".to_string();
    if let Some(cwd_val) = root.get("cwd") {
        if let Some(value) = cwd_val.as_str() {
            cwd = value.to_string();
        } else {
            errors.push(ConfigError::InvalidCwd {
                src: src.clone(),
                span: SourceSpan::new(
                    cwd_val.span.start.into(),
                    cwd_val.span.end - cwd_val.span.start,
                ),
            });
        }
    }

    // --- Layout Validation ---
    let mut layout = None;
    if let Some(layout_val) = root.get("layout") {
        let l_span = SourceSpan::new(
            layout_val.span.start.into(),
            layout_val.span.end - layout_val.span.start,
        );
        if let Some(l_str) = layout_val.as_str() {
            match resolve_layout(l_str) {
                Some(resolved_id) => {
                    layout = Some(resolved_id);
                }
                None => {
                    errors.push(ConfigError::InvalidLayout {
                        src: src.clone(),
                        layout: l_str.to_string(),
                        span: l_span,
                        suggestion: suggest_layout_fuzzy(l_str)
                            .map(|s| format!("Did you mean `{}`?", s)),
                    });
                }
            }
        }
    }

    // --- Menubar Validation ---
    let mut menubar = MenubarConfig {
        enabled: false,
        max_len: 32,
    };
    if let Some(menubar_val) = root.get("menubar") {
        let m_span = SourceSpan::new(
            menubar_val.span.start.into(),
            menubar_val.span.end - menubar_val.span.start,
        );
        if let Some(flag) = menubar_val.as_bool() {
            menubar.enabled = flag;
        } else if let Some(table) = menubar_val.as_table() {
            if let Some(enabled_val) = table.get("enabled") {
                if let Some(flag) = enabled_val.as_bool() {
                    menubar.enabled = flag;
                } else {
                    errors.push(ConfigError::InvalidMenubarField {
                        src: src.clone(),
                        field: "enabled".into(),
                        span: SourceSpan::new(
                            enabled_val.span.start.into(),
                            enabled_val.span.end - enabled_val.span.start,
                        ),
                        message: "menubar.enabled must be true or false".into(),
                    });
                }
            }
            if let Some(max_len_val) = table.get("max_len") {
                if let Some(v) = max_len_val.as_integer() {
                    if v > 0 {
                        menubar.max_len = v as usize;
                    } else {
                        errors.push(ConfigError::InvalidMenubarField {
                            src: src.clone(),
                            field: "max_len".into(),
                            span: SourceSpan::new(
                                max_len_val.span.start.into(),
                                max_len_val.span.end - max_len_val.span.start,
                            ),
                            message: "menubar.max_len must be a positive integer".into(),
                        });
                    }
                } else {
                    errors.push(ConfigError::InvalidMenubarField {
                        src: src.clone(),
                        field: "max_len".into(),
                        span: SourceSpan::new(
                            max_len_val.span.start.into(),
                            max_len_val.span.end - max_len_val.span.start,
                        ),
                        message: "menubar.max_len must be an integer".into(),
                    });
                }
            }
        } else {
            errors.push(ConfigError::InvalidMenubarField {
                src: src.clone(),
                field: "menubar".into(),
                span: m_span,
                message: "menubar must be a boolean or table".into(),
            });
        }
    }

    // --- Mods Validation ---
    let mut resolved_aliases: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();

    if let Some(table) = root.get("mods").and_then(|v| v.as_table()) {
        for (key, val) in table {
            let key_str = key.to_string();
            let key_span = SourceSpan::new(key.span.start.into(), key.span.end - key.span.start);

            if !Modifiers::parse(&key_str).is_empty() {
                errors.push(ConfigError::InvalidBinding {
                    src: src.clone(),
                    raw: key_str.clone(),
                    span: key_span,
                    message: format!(
                        "The name '{}' is a reserved modifier and cannot be used as an alias.",
                        key_str
                    ),
                });
                continue; // Skip this alias to avoid further confusion
            }

            let val_span = SourceSpan::new(val.span.start.into(), val.span.end - val.span.start);

            let raw_parts: Vec<&str> = match val.as_ref() {
                ValueInner::String(s) => s
                    .split(|c: char| c == '+' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .collect(),
                ValueInner::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
                _ => Vec::new(),
            };

            let mut modifiers = Modifiers::NONE;
            for &part in &raw_parts {
                let m = Modifiers::parse(part);
                if m.is_empty() {
                    // It's not a valid modifier! Check for typos.
                    // Note: We don't check ctx.modifier_names here because
                    // you can't define a modifier alias using another alias.
                    let suggestion = suggest_best_match(part, MODIFIER_SUGGESTIONS);

                    errors.push(ConfigError::InvalidBinding {
                        src: src.clone(),
                        raw: part.to_string(),
                        span: val_span, // Or more precisely, the sub-span if you have it
                        message: format!(
                            "Unrecognized modifier '{}' in alias definition. {}",
                            part,
                            suggestion
                                .map(|s| format!("Did you mean '{}'?", s))
                                .unwrap_or_default()
                        ),
                    });
                } else {
                    modifiers |= m;
                }
            }

            if modifiers.is_empty() {
                continue;
            }

            // Check for RedundantAlias... (rest of your logic)
            if let Some((original_name, original_span)) = resolved_aliases.get(&modifiers) {
                errors.push(ConfigError::RedundantAlias {
                    src: src.clone(),
                    alias1: original_name.clone(),
                    alias2: key.to_string(),
                    span1: *original_span,
                    span2: key_span,
                });
            } else {
                resolved_aliases.insert(modifiers, (key.to_string(), key_span));
            }
        }
    }

    // --- 3. Parse and Validate App Aliases ---
    let mut app_aliases = HashMap::new();
    let mut app_groups = HashMap::new();
    if let Some(apps_table) = root.get("apps").and_then(|v| v.as_table()) {
        for (key, val) in apps_table {
            let alias_key = key.to_string();
            match val.as_ref() {
                ValueInner::String(real_name) => {
                    let app_span =
                        SourceSpan::new(val.span.start.into(), val.span.end - val.span.start);

                    let is_invalid = real_name.is_empty()
                        || real_name.contains('/')
                        || real_name.trim().is_empty();

                    if is_invalid {
                        errors.push(ConfigError::InvalidAppName {
                            src: src.clone(),
                            name: real_name.to_string(),
                            span: app_span,
                            help: "Invalid app name".into(),
                        });
                        continue;
                    }

                    // Map: "chrome" -> "Google Chrome"
                    app_aliases.insert(alias_key, real_name.to_string());
                }
                ValueInner::Array(items) => {
                    let mut group = Vec::new();
                    for item in items {
                        let item_span = SourceSpan::new(
                            item.span.start.into(),
                            item.span.end - item.span.start,
                        );
                        let Some(name) = item.as_str() else {
                            errors.push(ConfigError::InvalidAppGroupEntry {
                                src: src.clone(),
                                group: alias_key.clone(),
                                span: item_span,
                                message: "Group entries must be strings".into(),
                            });
                            continue;
                        };
                        let is_invalid =
                            name.is_empty() || name.contains('/') || name.trim().is_empty();
                        if is_invalid {
                            errors.push(ConfigError::InvalidAppGroupEntry {
                                src: src.clone(),
                                group: alias_key.clone(),
                                span: item_span,
                                message: "Invalid app name in group".into(),
                            });
                            continue;
                        }
                        group.push(name.to_string());
                    }
                    app_groups.insert(alias_key, group);
                }
                _ => {
                    let app_span =
                        SourceSpan::new(val.span.start.into(), val.span.end - val.span.start);
                    errors.push(ConfigError::InvalidAppGroupEntry {
                        src: src.clone(),
                        group: alias_key.clone(),
                        span: app_span,
                        message: "App alias must be a string or array of strings".into(),
                    });
                }
            }
        }
    }

    let ctx = ValidationContext {
        src: &src,
        modifier_map: &resolved_aliases,
        modifier_names: resolved_aliases.values().map(|v| v.0.clone()).collect(),
        app_aliases,
        app_groups,
    };

    // --- 4. Parse Global Binds ---
    let mut global_binds = HashMap::new();
    if let Some(binds_table) = root.get("binds").and_then(|v| v.as_table()) {
        for (raw_key, val) in binds_table {
            let key_str = raw_key.to_string();
            let key_span = SourceSpan::new(
                raw_key.span.start.into(),
                raw_key.span.end - raw_key.span.start,
            );

            // 1. Parse the trigger (Key + Modifiers)
            let trigger = parse_keybinding(&key_str, key_span, &mut errors, &ctx);

            // 2. Parse the action (Single or Sequence)
            let action = parse_action(
                val,
                &mut errors,
                &ctx,
                ParseScope {
                    in_layer: false,
                    app_name: None,
                },
            );

            // 3. If both are valid, hydrate the map
            if let (Some(t), Some(a)) = (trigger, action) {
                global_binds.insert(t, a);
            }
        }
    }

    let layers = match root.get("layer").and_then(|v| v.as_table()) {
        Some(layers_table) => parse_layers(layers_table, &mut errors, &ctx, None),
        None => HashMap::new(),
    };

    let apps = match root.get("app").and_then(|v| v.as_table()) {
        Some(apps_table) => parse_apps(apps_table, &mut errors, &ctx, 0),
        None => Vec::new(),
    };

    if !errors.is_empty() {
        return Err(Report::new(MultiConfigError { src, errors }));
    }

    let config = Config {
        apps,
        cwd,
        global_binds,
        layers,
        layout,
        menubar,
    };

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::parse_config;
    use crate::config::action::{Action, LayerTargetScope, MenubarAction};
    use crate::config::layer::LayerMode;
    use std::path::PathBuf;

    #[test]
    fn cwd_defaults_to_kiwi_home() {
        let config = parse_config("", PathBuf::from("test.toml")).expect("config should parse");
        assert_eq!(config.cwd, "$KIWI");
    }

    #[test]
    fn cwd_parses_as_a_string() {
        let config = parse_config(r#"cwd = "$HOME/code""#, PathBuf::from("test.toml"))
            .expect("config should parse");
        assert_eq!(config.cwd, "$HOME/code");
    }

    #[test]
    fn cwd_rejects_non_string_values() {
        assert!(parse_config("cwd = 42", PathBuf::from("test.toml")).is_err());
    }

    #[test]
    fn layer_mode_defaults_to_oneshot_and_parses_deactivate() {
        let raw = r#"
[layer.main]
activate = "cmd+k"
deactivate = "esc"
"x" = "reload"
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let (_, layer) = config.layers.iter().next().expect("expected one layer");

        assert_eq!(layer.mode, LayerMode::Oneshot);
        assert!(layer.deactivate.is_some());
    }

    #[test]
    fn layer_mode_sticky_and_timeout_zero_parse() {
        let raw = r#"
[layer.main]
activate = "cmd+k"
mode = "sticky"
timeout = 0
"x" = "reload"
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let (_, layer) = config.layers.iter().next().expect("expected one layer");

        assert_eq!(layer.mode, LayerMode::Sticky);
        assert_eq!(layer.timeout, Some(0));
    }

    #[test]
    fn invalid_layer_mode_is_rejected() {
        let raw = r#"
[layer.main]
activate = "cmd+k"
mode = "invalid"
"x" = "reload"
"#;

        let err = parse_config(raw, PathBuf::from("test.toml"));
        assert!(err.is_err());
    }

    #[test]
    fn media_key_cannot_be_used_as_binding_trigger() {
        let raw = r#"
[binds]
"missioncontrol" = "reload"
"#;

        let err = parse_config(raw, PathBuf::from("test.toml"));
        assert!(err.is_err());
    }

    #[test]
    fn media_key_can_be_used_as_binding_trigger() {
        let raw = r#"
[binds]
"volumeup" = "reload"
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        assert_eq!(config.global_binds.len(), 1);
    }

    #[test]
    fn remap_can_target_media_key_with_modifiers() {
        let raw = r#"
[binds]
"cmd+k" = "remap:cmd+shift+volumeup"
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        assert_eq!(config.global_binds.len(), 1);
    }

    #[test]
    fn remap_can_target_keyboard_brightness_aliases() {
        let raw = r#"
[binds]
"cmd+k" = "remap:kbup"
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        assert_eq!(config.global_binds.len(), 1);
    }

    #[test]
    fn bare_binding_action_parses_as_send_key() {
        let raw = r#"
[binds]
"cmd+sft+c" = ["cmd+l", "cmd+c", "esc", "esc"]
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let action = config
            .global_binds
            .values()
            .next()
            .expect("expected action");
        match action {
            Action::Sequence(items) => {
                assert!(items.iter().all(|a| matches!(a, Action::SendKey(_))));
            }
            _ => panic!("expected action sequence"),
        }
    }

    #[test]
    fn repeat_and_layer_actions_parse_in_layer_scope() {
        let raw = r#"
[layer.media]
activate = "cmd+m"
"j" = ["repeat(brdn, 16)", "pop", "layer:root", "layer:launch"]
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let layer = config.layers.values().next().expect("layer");
        let action = layer.binds.values().next().expect("action");
        match action {
            Action::Sequence(items) => {
                assert!(matches!(items[0], Action::Repeat { .. }));
                assert!(matches!(items[1], Action::LayerPop));
                assert!(matches!(items[2], Action::LayerRoot));
                match &items[3] {
                    Action::LayerActivate { target, scope } => {
                        assert_eq!(target, "launch");
                        assert_eq!(scope, &LayerTargetScope::GlobalOnly);
                    }
                    _ => panic!("expected layer activate"),
                }
            }
            _ => panic!("expected sequence action"),
        }
    }

    #[test]
    fn layer_action_is_rejected_outside_layer_scope() {
        let raw = r#"
[app.Ghostty]
"cmd+j" = "layer:kiwi"
"#;
        assert!(parse_config(raw, PathBuf::from("test.toml")).is_err());
    }

    #[test]
    fn app_group_alias_parses_into_selector() {
        let raw = r#"
[apps]
tabs = ["Ghostty", "Safari", "Terminal"]

[app.tabs]
"cmd+t" = "reload"
"#;
        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        assert_eq!(config.apps.len(), 1);
        let entry = &config.apps[0];
        assert_eq!(entry.label, "tabs");
        match &entry.selector {
            super::AppSelector::Any(items) => {
                assert_eq!(
                    items,
                    &vec![
                        "Ghostty".to_string(),
                        "Safari".to_string(),
                        "Terminal".to_string()
                    ]
                );
            }
            _ => panic!("expected any selector"),
        }
    }

    #[test]
    fn any_selector_expands_groups_and_names() {
        let raw = r#"
[apps]
tabs = ["Ghostty", "Safari"]

[app."any(tabs, Terminal)"]
"cmd+t" = "reload"
"#;
        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let entry = &config.apps[0];
        match &entry.selector {
            super::AppSelector::Any(items) => {
                assert_eq!(
                    items,
                    &vec![
                        "Ghostty".to_string(),
                        "Safari".to_string(),
                        "Terminal".to_string()
                    ]
                );
            }
            _ => panic!("expected any selector"),
        }
    }

    #[test]
    fn not_selector_parses_group() {
        let raw = r#"
[apps]
tabs = ["Ghostty"]

[app."not(tabs)"]
"cmd+t" = "reload"
"#;
        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let entry = &config.apps[0];
        match &entry.selector {
            super::AppSelector::Not(inner) => match inner.as_ref() {
                super::AppSelector::Any(items) => {
                    assert_eq!(items, &vec!["Ghostty".to_string()]);
                }
                _ => panic!("expected any selector inside not"),
            },
            _ => panic!("expected not selector"),
        }
    }

    #[test]
    fn invalid_any_selector_errors() {
        let raw = r#"
[app."any()"]
"cmd+t" = "reload"
"#;
        assert!(parse_config(raw, PathBuf::from("test.toml")).is_err());
    }

    #[test]
    fn type_action_parses_from_prefix() {
        let raw = r#"
[binds]
"cmd+t" = "type:Hello from Unicode injection 🚀"
"#;
        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let action = config
            .global_binds
            .values()
            .next()
            .expect("expected action");
        match action {
            Action::Type(text) => {
                assert_eq!(text, "Hello from Unicode injection 🚀");
            }
            _ => panic!("expected type action"),
        }
    }

    #[test]
    fn menubar_table_action_parses() {
        let raw = r#"
[binds]
"cmd+c" = { action = "menubar:click", item = ["Edit", "Copy"] }
"fn+cmd+c" = { action = "menubar:show", app = "Safari", item = ["Edit", "Copy"] }
"cmd+u" = { action = "menubar:click", app = "chrome", item = ["File", "New Tab"] }
"cmd+shift+u" = { action = "menubar:show", item = ["Copy"] }
"#;
        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");

        assert_eq!(config.global_binds.len(), 4);

        let click = config
            .global_binds
            .values()
            .find(|action| {
                matches!(
                    action,
                    Action::MenubarClick(MenubarAction { app: None, .. })
                )
            })
            .expect("expected default-app click action");
        match click {
            Action::MenubarClick(MenubarAction { app, item }) => {
                assert_eq!(app, &None);
                assert_eq!(item, &vec!["Edit".to_string(), "Copy".to_string()]);
            }
            _ => panic!("expected menubar click action"),
        }

        let show = config
            .global_binds
            .values()
            .find(|action| {
                matches!(
                    action,
                    Action::MenubarShow(MenubarAction { app: Some(app), .. }) if app == "Safari"
                )
            })
            .expect("expected Safari show action");
        match show {
            Action::MenubarShow(MenubarAction { app, item }) => {
                assert_eq!(app, &Some("Safari".to_string()));
                assert_eq!(item, &vec!["Edit".to_string(), "Copy".to_string()]);
            }
            _ => panic!("expected menubar show action"),
        }
    }

    #[test]
    fn malformed_menubar_table_is_rejected() {
        let raw = r#"
[binds]
"cmd+c" = { action = "menubar:click", item = 42 }
"#;

        assert!(parse_config(raw, PathBuf::from("test.toml")).is_err());
    }

    #[test]
    fn structured_menubar_action_parses_inside_layer() {
        let raw = r#"
[layer.infuse]
activate = "cmd+u"
spc = { action = "menubar:click", app = "Infuse", item = "Play/Pause" }
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let layer = config.layers.values().next().expect("expected layer");
        let action = layer.binds.values().next().expect("expected action");
        match action {
            Action::MenubarClick(MenubarAction { app, item }) => {
                assert_eq!(app, &Some("Infuse".to_string()));
                assert_eq!(item, &vec!["Play/Pause".to_string()]);
            }
            _ => panic!("expected menubar click action"),
        }
    }

    #[test]
    fn structured_menubar_action_parses_inside_app() {
        let raw = r#"
[app."Infuse"]
"spc" = { action = "menubar:click", app = "Infuse", item = ["Play/Pause"] }
"#;

        let config = parse_config(raw, PathBuf::from("test.toml")).expect("config should parse");
        let entry = &config.apps[0];
        let action = entry.app.binds.values().next().expect("expected action");
        match action {
            Action::MenubarClick(MenubarAction { app, item }) => {
                assert_eq!(app, &Some("Infuse".to_string()));
                assert_eq!(item, &vec!["Play/Pause".to_string()]);
            }
            _ => panic!("expected menubar click action"),
        }
    }
}
