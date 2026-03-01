use crate::{
    config::{
        ValidationContext,
        action::{Action, ParseScope, parse_action},
        binding::parse_keybinding,
        error::ConfigError,
        utils::is_similar,
    },
    key::KeyBinding,
};
use miette::SourceSpan;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Layer {
    pub name: String,
    pub mode: LayerMode,
    pub timeout: Option<u32>,
    pub deactivate: Option<KeyBinding>,
    pub binds: HashMap<KeyBinding, Action>,
    pub children: HashMap<KeyBinding, Layer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerMode {
    Oneshot,
    Sticky,
}

impl LayerMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "oneshot" => Some(Self::Oneshot),
            "sticky" => Some(Self::Sticky),
            _ => None,
        }
    }
}

pub fn parse_layers(
    table: &toml_span::value::Table,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    app_name: Option<&str>,
) -> HashMap<KeyBinding, Layer> {
    let reserved = ["activate", "timeout", "mode", "deactivate"];
    let mut layers: HashMap<KeyBinding, Layer> = HashMap::new();

    for (key, val) in table {
        let key_str = key.to_string();
        let key_span = SourceSpan::new(
            key.span.start.into(),
            key.span.end - key.span.start,
        );

        if let Some(inner_table) = val.as_table() {
            let mut layer_binds = HashMap::new();
            let mut activate_trigger = None;
            let mut timeout_ms = None;
            let mut layer_mode = LayerMode::Oneshot;
            let mut deactivate_trigger = None;

            // 1. Requirement check: Nested tables must have 'activate'
            if !inner_table.contains_key("activate") {
                errors.push(ConfigError::MissingField {
                    src: ctx.src.clone(),
                    field: "activate".into(),
                    table_type: format!("layer '{}'", key_str),
                    span: key_span,
                });
            }

            // 2. Process the table contents
            for (i_key, i_val) in inner_table {
                let i_key_str = i_key.to_string();
                let i_key_span = SourceSpan::new(
                    i_key.span.start.into(),
                    i_key.span.end - i_key.span.start,
                );

                if i_val.as_table().is_some() {
                    continue;
                }

                match i_key_str.as_str() {
                    "activate" => {
                        if let Some(raw_s) = i_val.as_str() {
                            activate_trigger = parse_keybinding(raw_s, i_key_span, errors, ctx);
                        }
                    }
                    "timeout" => {
                        timeout_ms = parse_timeout_field(i_val, errors, ctx);
                    }
                    "mode" => {
                        if let Some(raw_mode) = i_val.as_str() {
                            if let Some(parsed_mode) = LayerMode::parse(raw_mode) {
                                layer_mode = parsed_mode;
                            } else {
                                errors.push(ConfigError::InvalidBinding {
                                    src: ctx.src.clone(),
                                    raw: raw_mode.to_string(),
                                    span: i_key_span,
                                    message: "Invalid layer mode. Valid values: oneshot, sticky"
                                        .into(),
                                });
                            }
                        } else {
                            errors.push(ConfigError::InvalidBinding {
                                src: ctx.src.clone(),
                                raw: i_key_str.clone(),
                                span: i_key_span,
                                message: "Layer mode must be a string".into(),
                            });
                        }
                    }
                    "deactivate" => {
                        if let Some(raw_s) = i_val.as_str() {
                            deactivate_trigger = parse_keybinding(raw_s, i_key_span, errors, ctx);
                        } else {
                            errors.push(ConfigError::InvalidBinding {
                                src: ctx.src.clone(),
                                raw: i_key_str.clone(),
                                span: i_key_span,
                                message: "deactivate must be a keybinding string".into(),
                            });
                        }
                    }
                    _ => {
                        // Check for typos of reserved words (e.g., "activte")
                        for target in reserved {
                            if is_similar(&i_key_str, target) {
                                errors.push(ConfigError::UnknownField {
                                    src: ctx.src.clone(),
                                    found: i_key_str.clone(),
                                    span: i_key_span,
                                    help: format!("Did you mean '{}'?", target),
                                });
                            }
                        }

                        // It's a binding. Try to parse both sides.
                        if let Some(trigger) =
                            parse_keybinding(&i_key_str, i_key_span, errors, ctx)
                            && let Some(action) = parse_action(
                                i_val,
                                errors,
                                ctx,
                                ParseScope {
                                    in_layer: true,
                                    app_name,
                                },
                            )
                        {
                            layer_binds.insert(trigger, action);
                        }
                    }
                }
            }

            // 3. Handle Nested Layers (Recursion)
            // We search the inner table for any values that are themselves tables
            // let children = parse_layers(inner_table, errors, ctx);
            // if let Some(activate_trigger) = activate_trigger {
            //     layers.insert(activate_trigger, Layer {
            //         name: key_str,
            //         // activate: activate_trigger,
            //         mode: layer_mode,
            //         timeout: timeout_ms,
            //         binds: layer_binds,
            //         children,
            //     });
            // }
            // 3. Handle Nested Layers (Recursion)
            let children = parse_layers(inner_table, errors, ctx, app_name);

            if let Some(trigger) = activate_trigger {
                // --- DUPLICATE CHECK START ---
                if let Some(existing_layer) = layers.get(&trigger) {
                    errors.push(ConfigError::InvalidBinding {
                        src: ctx.src.clone(),
                        raw: format!("{:?}", trigger),
                        span: key_span, // The span of the layer name 'key_str'
                        message: format!(
                            "Layer activation conflict: '{:?}' is already used by layer '{}'.",
                            trigger, existing_layer.name
                        ),
                    });
                    // We continue because we don't want to overwrite the existing layer
                    continue;
                }
                // --- DUPLICATE CHECK END ---

                layers.insert(
                    trigger,
                    Layer {
                        name: key_str,
                        mode: layer_mode,
                        timeout: timeout_ms,
                        deactivate: deactivate_trigger,
                        binds: layer_binds,
                        children,
                    },
                );
            }
            // 4. Construct the Layer object
        }
    }
    layers
}

fn parse_timeout_field(
    val: &toml_span::Value,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<u32> {
    let span = SourceSpan::new(
        val.span.start.into(),
        val.span.end - val.span.start,
    );

    if let Some(int_val) = val.as_integer() {
        if int_val >= 0 {
            return Some(int_val as u32);
        } else {
            errors.push(ConfigError::InvalidTimeout {
                src: ctx.src.clone(),
                span,
            });
        }
    } else if let Some(s_val) = val.as_str() && let Ok(parsed) = s_val.parse::<u32>() {
        errors.push(ConfigError::TimeoutCoercion {
            src: ctx.src.clone(),
            span,
            val: s_val.to_string(),
            parsed: parsed as i64,
            help: format!("Consider changing to: timeout = {}", parsed),
        });
        return Some(parsed);
    }
    None
}
