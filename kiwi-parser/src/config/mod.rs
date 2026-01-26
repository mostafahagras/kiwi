pub mod action;
mod app;
pub mod binding;
pub mod error;
mod layer;
mod utils;

use crate::{
    config::{
        action::{Action, parse_action},
        app::{App, parse_apps},
        binding::parse_keybinding,
        error::{ConfigError, MultiConfigError},
        layer::Layer,
        layer::parse_layers,
        utils::MODIFIER_SUGGESTIONS,
    },
    key::{KeyBinding, Modifiers},
    layout::{layout_exists, suggest_layout_fuzzy},
};
use miette::{NamedSource, Report, SourceSpan};
use std::{collections::HashMap, path::PathBuf};
use toml_span::{parse, value::ValueInner};
use utils::suggest_best_match;

#[derive(Debug)]
pub struct Config {
    pub layout: Option<String>,
    pub global_binds: HashMap<KeyBinding, Action>,
    pub layers: HashMap<KeyBinding, Layer>,
    pub apps: HashMap<String, App>,
}

pub struct ValidationContext<'a> {
    pub src: &'a NamedSource<String>,
    /// Maps sorted modifiers to their alias name (e.g., [Ctrl, Alt] -> "meh")
    pub modifier_map: &'a HashMap<Modifiers, (String, SourceSpan)>,
    /// Simple list of alias names for fuzzy matching (e.g., ["hyper", "meh"])
    pub modifier_names: Vec<String>,
    pub app_aliases: HashMap<String, String>,
}

pub fn parse_config(raw_toml: &str, path: PathBuf) -> Result<Config, Report> {
    let src = NamedSource::new(path.to_str().unwrap(), raw_toml.to_string());

    // 1. Handle TOML Syntax errors (like duplicate keys)
    let doc = match parse(raw_toml) {
        Ok(d) => d,
        Err(e) => {
            let span = SourceSpan::new(e.span.start.into(), (e.span.end - e.span.start).into());
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

    // --- Layout Validation ---
    let mut layout = None;
    if let Some(layout_val) = root.get("layout") {
        let l_span = SourceSpan::new(
            layout_val.span.start.into(),
            (layout_val.span.end - layout_val.span.start).into(),
        );
        if let Some(l_str) = layout_val.as_str() {
            if !layout_exists(l_str) {
                errors.push(ConfigError::InvalidLayout {
                    src: src.clone(),
                    layout: l_str.to_string(),
                    span: l_span,
                    suggestion: suggest_layout_fuzzy(l_str)
                        .map(|s| format!("Did you mean `{}`?", s)),
                });
            }
            layout = Some(l_str.to_string());
        }
    }

    // --- Mods Validation ---
    let mut resolved_aliases: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();

    if let Some(table) = root.get("mods").and_then(|v| v.as_table()) {
        for (key, val) in table {
            let key_str = key.to_string();
            let key_span = SourceSpan::new(
                key.span.start.into(),
                (key.span.end - key.span.start).into(),
            );

            if !Modifiers::from_str(&key_str).is_empty() {
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

            let val_span = SourceSpan::new(
                val.span.start.into(),
                (val.span.end - val.span.start).into(),
            );

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
                let m = Modifiers::from_str(part);
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
    if let Some(apps_table) = root.get("apps").and_then(|v| v.as_table()) {
        for (key, val) in apps_table {
            let alias_key = key.to_string();
            if let Some(real_name) = val.as_str() {
                let app_span = SourceSpan::new(
                    val.span.start.into(),
                    (val.span.end - val.span.start).into(),
                );

                // Validation logic
                let is_invalid =
                    real_name.is_empty() || real_name.contains('/') || real_name.trim().is_empty();

                if is_invalid {
                    errors.push(ConfigError::InvalidAppName {
                        src: src.clone(),
                        name: real_name.to_string(),
                        span: app_span,
                        help: "Invalid app name".into(),
                    });
                }

                // Map: "chrome" -> "Google Chrome"
                app_aliases.insert(alias_key, real_name.to_string());
            }
        }
    }

    let ctx = ValidationContext {
        src: &src,
        modifier_map: &resolved_aliases,
        modifier_names: resolved_aliases.values().map(|v| v.0.clone()).collect(),
        app_aliases,
    };

    // --- 4. Parse Global Binds ---
    let mut global_binds = HashMap::new();
    if let Some(binds_table) = root.get("binds").and_then(|v| v.as_table()) {
        for (raw_key, val) in binds_table {
            let key_str = raw_key.to_string();
            let key_span = SourceSpan::new(
                raw_key.span.start.into(),
                (raw_key.span.end - raw_key.span.start).into(),
            );

            // 1. Parse the trigger (Key + Modifiers)
            let trigger = parse_keybinding(&key_str, key_span, &mut errors, &ctx);

            // 2. Parse the action (Single or Sequence)
            let action = parse_action(val, &mut errors, &ctx);

            // 3. If both are valid, hydrate the map
            if let (Some(t), Some(a)) = (trigger, action) {
                global_binds.insert(t, a);
            }
        }
    }

    let layers = match root.get("layer").and_then(|v| v.as_table()) {
        Some(layers_table) => parse_layers(layers_table, &mut errors, &ctx),
        None => HashMap::new(),
    };

    let apps = match root.get("app").and_then(|v| v.as_table()) {
        Some(apps_table) => parse_apps(apps_table, &mut errors, &ctx, 0),
        None => HashMap::new(),
    };

    if !errors.is_empty() {
        return Err(Report::new(MultiConfigError { src, errors }));
    }

    let config = Config {
        apps,
        global_binds,
        layers,
        layout,
    };

    Ok(config)
}
