use crate::{
    config::{
        ValidationContext,
        action::{Action, ParseScope, parse_action},
        binding::parse_keybinding,
        error::ConfigError, layer::Layer, layer::parse_layers,
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

pub fn parse_apps(
    table: &toml_span::value::Table,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    depth: usize,
) -> HashMap<String, App> {
    let mut apps = HashMap::new();

    for (key, val) in table {
        let key_str = key.to_string();
        let resolved_name = ctx.app_aliases.get(&key_str).cloned().unwrap_or(key_str);
        let key_span = SourceSpan::new(
            key.span.start.into(),
            key.span.end - key.span.start,
        );

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
                let i_key_span = SourceSpan::new(
                    i_key.span.start.into(),
                    i_key.span.end - i_key.span.start,
                );

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

            apps.insert(
                resolved_name,
                App {
                    binds: app_binds,
                    children,
                },
            );
        }
    }
    apps
}
