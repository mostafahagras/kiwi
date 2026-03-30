use crate::{
    config::{
        ValidationContext,
        action::{Action, ParseScope, parse_action},
        binding::parse_keybinding,
        error::ConfigError,
        layer::Layer,
        layer::parse_layers,
    },
    key::KeyBinding,
};
use miette::SourceSpan;
use std::collections::HashMap;

#[derive(Debug)]
pub struct App {
    pub binds: HashMap<KeyBinding, Action>,
    pub children: HashMap<KeyBinding, Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppSelector {
    Exact(String),
    Any(Vec<String>),
    Not(Box<AppSelector>),
}

impl AppSelector {
    pub fn matches(&self, app: &str) -> bool {
        match self {
            AppSelector::Exact(name) => name == app,
            AppSelector::Any(names) => names.iter().any(|name| name == app),
            AppSelector::Not(inner) => !inner.matches(app),
        }
    }

    pub fn specificity(&self) -> u8 {
        match self {
            AppSelector::Exact(_) => 2,
            AppSelector::Any(_) => 1,
            AppSelector::Not(_) => 0,
        }
    }
}

#[derive(Debug)]
pub struct AppEntry {
    pub label: String,
    pub selector: AppSelector,
    pub app: App,
}

pub fn parse_apps(
    table: &toml_span::value::Table,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    depth: usize,
) -> Vec<AppEntry> {
    let mut apps = Vec::new();

    for (key, val) in table {
        let key_str = key.to_string();
        let key_span = SourceSpan::new(key.span.start.into(), key.span.end - key.span.start);
        let Some(selector) = parse_app_selector(&key_str, key_span, errors, ctx) else {
            continue;
        };
        let label = key_str.clone();
        let resolved_name = label.clone();

        if let Some(inner_table) = val.as_table() {
            let mut app_binds = HashMap::new();

            // 1. Requirement check: Only require 'activate' for nested modes (depth > 0)
            if depth > 0 && !inner_table.contains_key("activate") {
                errors.push(ConfigError::MissingField {
                    src: ctx.src.clone(),
                    field: "activate".into(),
                    table_type: format!("app mode '{}'", resolved_name),
                    span: key_span,
                });
            }

            // 2. Process table contents
            for (i_key, i_val) in inner_table {
                let i_key_str = i_key.to_string();
                let i_key_span =
                    SourceSpan::new(i_key.span.start.into(), i_key.span.end - i_key.span.start);

                // Skip child tables; handled by recursion
                if i_val.as_table().is_some() {
                    continue;
                }

                // Apps don't strictly have "reserved" fields like timeout/mode
                // at the top level, but we skip 'activate' as a bind if it exists.
                if i_key_str == "activate" {
                    continue;
                }

                // Parse as a binding
                if let Some(trigger) = parse_keybinding(&i_key_str, i_key_span, errors, ctx)
                    && let Some(action) = parse_action(
                        i_val,
                        errors,
                        ctx,
                        ParseScope {
                            in_layer: false,
                            app_name: Some(&resolved_name),
                        },
                    )
                {
                    app_binds.insert(trigger, action);
                }
            }

            // 3. Handle Nesting
            // IMPORTANT: Your struct says App.children is Vec<Layer>.
            // So we use parse_layers for anything nested inside an App.
            let children = parse_layers(inner_table, errors, ctx, Some(&resolved_name));

            apps.push(AppEntry {
                label,
                selector,
                app: App {
                    binds: app_binds,
                    children,
                },
            });
        }
    }
    apps
}

fn parse_app_selector(
    raw: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<AppSelector> {
    let raw_trimmed = raw.trim();
    if raw_trimmed.starts_with("any(") {
        if !raw_trimmed.ends_with(')') {
            errors.push(ConfigError::InvalidAppSelector {
                src: ctx.src.clone(),
                selector: raw_trimmed.to_string(),
                span,
                help: "any() selector must end with ')'".into(),
            });
            return None;
        }
        let inner = &raw_trimmed[4..raw_trimmed.len() - 1];
        return parse_any_selector(raw_trimmed, inner, span, errors, ctx);
    }
    if raw_trimmed.starts_with("not(") {
        if !raw_trimmed.ends_with(')') {
            errors.push(ConfigError::InvalidAppSelector {
                src: ctx.src.clone(),
                selector: raw_trimmed.to_string(),
                span,
                help: "not() selector must end with ')'".into(),
            });
            return None;
        }
        let inner = &raw_trimmed[4..raw_trimmed.len() - 1];
        return parse_not_selector(raw_trimmed, inner, span, errors, ctx);
    }

    if let Some(real_name) = ctx.app_aliases.get(raw_trimmed) {
        return Some(AppSelector::Exact(real_name.clone()));
    }

    if let Some(group) = ctx.app_groups.get(raw_trimmed) {
        return Some(AppSelector::Any(group.clone()));
    }

    if is_invalid_app_name(raw_trimmed) {
        errors.push(ConfigError::InvalidAppName {
            src: ctx.src.clone(),
            name: raw_trimmed.to_string(),
            span,
            help: "Invalid app name".into(),
        });
        return None;
    }

    Some(AppSelector::Exact(raw_trimmed.to_string()))
}

fn parse_any_selector(
    raw: &str,
    inner: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<AppSelector> {
    let items = parse_selector_list(raw, inner, span, errors, ctx)?;
    Some(AppSelector::Any(items))
}

fn parse_not_selector(
    raw: &str,
    inner: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<AppSelector> {
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        errors.push(ConfigError::InvalidAppSelector {
            src: ctx.src.clone(),
            selector: raw.to_string(),
            span,
            help: "not() requires a selector or app name".into(),
        });
        return None;
    }
    let inner_selector = parse_app_selector(inner_trimmed, span, errors, ctx)?;
    Some(AppSelector::Not(Box::new(inner_selector)))
}

fn parse_selector_list(
    raw: &str,
    inner: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<Vec<String>> {
    let mut items = Vec::new();
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        errors.push(ConfigError::InvalidAppSelector {
            src: ctx.src.clone(),
            selector: raw.to_string(),
            span,
            help: "any() requires at least one item".into(),
        });
        return None;
    }

    for part in inner_trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            errors.push(ConfigError::InvalidAppSelector {
                src: ctx.src.clone(),
                selector: raw.to_string(),
                span,
                help: "any() items cannot be empty".into(),
            });
            return None;
        }

        if let Some(group) = ctx.app_groups.get(token) {
            items.extend(group.iter().cloned());
            continue;
        }

        if let Some(real_name) = ctx.app_aliases.get(token) {
            items.push(real_name.clone());
            continue;
        }

        if is_invalid_app_name(token) {
            errors.push(ConfigError::InvalidAppName {
                src: ctx.src.clone(),
                name: token.to_string(),
                span,
                help: "Invalid app name".into(),
            });
            return None;
        }

        items.push(token.to_string());
    }

    Some(items)
}

fn is_invalid_app_name(name: &str) -> bool {
    name.is_empty() || name.contains('/') || name.trim().is_empty()
}
