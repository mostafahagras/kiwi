pub mod resize;
pub mod snap;

use crate::config::{
    ValidationContext,
    binding::{parse_keybinding, parse_remap_keybinding},
    error::ConfigError,
};
use crate::key::{KeyBinding, Modifiers};
use miette::{NamedSource, SourceSpan};
use std::collections::HashMap;
pub use resize::Resize;
pub use snap::Snap;
use std::time::{Duration, Instant};
use toml_span::value::ValueInner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerTargetScope {
    GlobalOnly,
    App(String),
}

#[derive(Debug, Clone, Copy)]
pub struct ParseScope<'a> {
    pub in_layer: bool,
    pub app_name: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Executes any shell command.
    /// Use with caution
    Shell(String),
    /// Remaps a keybinding to another
    Remap(KeyBinding),
    /// Sends a keybinding as input
    SendKey(KeyBinding),
    /// Snaps the window to a predefined position
    Snap(Snap),
    /// Changes the window size
    Resize(Resize),
    /// Live config reloading
    Reload,
    /// Clean up state and quit Kiwi
    Quit,
    /// Passes all user input for the specified duration
    SleepFor(Duration),
    /// Passes all user input until the specified instant
    /// Possible usage: SleepUntil(tomorrow)
    SleepUntil(Instant),
    /// Swallows all user input until binding is intercepted.
    /// Useful for cleaning the keyboard
    Swallow(KeyBinding),
    /// Passes all user input until binding is intercepted.
    /// Useful for temporarily disabling Kiwi
    Pass(KeyBinding),
    /// Execute multiple actions in sequence
    Sequence(Vec<Action>),
    /// Repeat sending a keybinding N times with optional delay
    Repeat {
        binding: KeyBinding,
        count: u32,
        delay_ms: u64,
    },
    /// Pop one active layer
    LayerPop,
    /// Clear active layers
    LayerRoot,
    /// Activate a layer target with scoped lookup
    LayerActivate {
        target: String,
        scope: LayerTargetScope,
    },
}

pub fn parse_action(
    value: &toml_span::Value,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    scope: ParseScope<'_>,
) -> Option<Action> {
    let span = SourceSpan::new(
        value.span.start.into(),
        value.span.end - value.span.start,
    );

    match value.as_ref() {
        // Case A: A single action string (e.g., "snap:left")
        ValueInner::String(raw_value) => {
            parse_single_action_string(raw_value, span, errors, ctx, scope)
        }

        // Case B: A list of actions (e.g., ["shell:say hi", "sleep:500", "quit"])
        ValueInner::Array(arr) => {
            let mut actions = Vec::with_capacity(arr.len());
            for item in arr {
                if let Some(action) = parse_action(item, errors, ctx, scope) {
                    actions.push(action);
                }
            }
            if actions.is_empty() {
                None
            } else {
                Some(Action::Sequence(actions))
            }
        }

        _ => {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: "Unsupported TOML type for action".into(),
                span,
                message: "Action must be a string or an array of strings".into(),
            });
            None
        }
    }
}

fn parse_single_action_string(
    raw_value: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    scope: ParseScope<'_>,
) -> Option<Action> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed == "pop" {
        if !scope.in_layer {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw_value.to_string(),
                span,
                message: "'pop' is only allowed inside [layer.*] bindings".into(),
            });
            return None;
        }
        return Some(Action::LayerPop);
    }

    if let Some(repeat) = parse_repeat_call(trimmed, span, errors, ctx) {
        return Some(repeat);
    }

    if let Some(target) = trimmed.strip_prefix("layer:") {
        if !scope.in_layer {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw_value.to_string(),
                span,
                message: "'layer:*' actions are only allowed inside [layer.*] bindings".into(),
            });
            return None;
        }

        let target = target.trim();
        if target.is_empty() {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw_value.to_string(),
                span,
                message: "layer target cannot be empty".into(),
            });
            return None;
        }

        if target == "root" {
            return Some(Action::LayerRoot);
        }

        let resolved_scope = match scope.app_name {
            Some(name) => LayerTargetScope::App(name.to_string()),
            None => LayerTargetScope::GlobalOnly,
        };
        return Some(Action::LayerActivate {
            target: target.to_string(),
            scope: resolved_scope,
        });
    }

    if let Some((prefix, payload)) = trimmed.split_once(':') {
        let payload = payload.trim();
        match prefix {
            "shell" => Some(Action::Shell(payload.to_string())),
            "remap" => parse_remap_keybinding(payload, span, errors, ctx).map(Action::Remap),
            "snap" => Snap::try_from(payload)
                .map(Action::Snap)
                .map_err(|e| {
                    errors.push(ConfigError::InvalidBinding {
                        src: ctx.src.clone(),
                        raw: raw_value.to_string(),
                        span,
                        message: format!("Invalid snap position: {}", e),
                    })
                })
                .ok(),
            "resize" => Resize::try_from(payload)
                .map(Action::Resize)
                .map_err(|e| {
                    errors.push(ConfigError::InvalidBinding {
                        src: ctx.src.clone(),
                        raw: raw_value.to_string(),
                        span,
                        message: format!("Invalid resize dimensions: {}", e),
                    })
                })
                .ok(),
            "sleep" => payload
                .parse::<u64>()
                .map(|ms| Action::SleepFor(Duration::from_millis(ms)))
                .map_err(|_| {
                    errors.push(ConfigError::InvalidBinding {
                        src: ctx.src.clone(),
                        raw: raw_value.to_string(),
                        span,
                        message: "Sleep requires milliseconds (e.g., sleep:500)".into(),
                    })
                })
                .ok(),
            "swallow" => parse_keybinding(payload, span, errors, ctx).map(Action::Swallow),
            "pass" => parse_keybinding(payload, span, errors, ctx).map(Action::Pass),
            _ => {
                errors.push(ConfigError::UnknownField {
                    src: ctx.src.clone(),
                    found: prefix.to_string(),
                    span,
                    help: "Valid prefixes: shell, remap, snap, resize, sleep, swallow, pass, layer".into(),
                });
                None
            }
        }
    } else {
        if let Some(binding) = try_parse_remap_keybinding(trimmed, span, ctx) {
            return Some(Action::SendKey(binding));
        }

        match trimmed {
            "reload" => Some(Action::Reload),
            "quit" => Some(Action::Quit),
            _ => Some(Action::Shell(trimmed.to_string())),
        }
    }
}

fn try_parse_remap_keybinding(
    raw: &str,
    span: SourceSpan,
    ctx: &ValidationContext,
) -> Option<KeyBinding> {
    let mut ignored = Vec::new();
    parse_remap_keybinding(raw, span, &mut ignored, ctx)
}

fn parse_repeat_call(
    raw: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<Action> {
    let Some(inner) = raw.strip_prefix("repeat(") else {
        return None;
    };
    let Some(inner) = inner.strip_suffix(')') else {
        errors.push(ConfigError::InvalidBinding {
            src: ctx.src.clone(),
            raw: raw.to_string(),
            span,
            message: "repeat(...) is missing closing ')'".into(),
        });
        return None;
    };

    let args: Vec<String> = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
        .collect();

    if !(2..=3).contains(&args.len()) {
        errors.push(ConfigError::InvalidBinding {
            src: ctx.src.clone(),
            raw: raw.to_string(),
            span,
            message: "repeat expects 2 or 3 args: repeat(binding, count[, delay_ms])".into(),
        });
        return None;
    }

    let Some(binding) = parse_remap_keybinding(&args[0], span, errors, ctx) else {
        return None;
    };

    let Ok(count) = args[1].parse::<u32>() else {
        errors.push(ConfigError::InvalidBinding {
            src: ctx.src.clone(),
            raw: raw.to_string(),
            span,
            message: "repeat count must be a positive integer".into(),
        });
        return None;
    };
    if count == 0 {
        errors.push(ConfigError::InvalidBinding {
            src: ctx.src.clone(),
            raw: raw.to_string(),
            span,
            message: "repeat count must be greater than 0".into(),
        });
        return None;
    }

    let delay_ms = if args.len() == 3 {
        let Ok(delay) = args[2].parse::<u64>() else {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw.to_string(),
                span,
                message: "repeat delay_ms must be a non-negative integer".into(),
            });
            return None;
        };
        delay
    } else {
        0
    };

    Some(Action::Repeat {
        binding,
        count,
        delay_ms,
    })
}

pub fn parse_action_str(raw: &str) -> Result<Action, String> {
    let src = NamedSource::new("ctl-action", raw.to_string());
    let modifier_map: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();
    let ctx = ValidationContext {
        src: &src,
        modifier_map: &modifier_map,
        modifier_names: Vec::new(),
        app_aliases: HashMap::new(),
    };

    let mut errors = Vec::new();
    let span = SourceSpan::new(0.into(), raw.len());
    let action = parse_single_action_string(
        raw,
        span,
        &mut errors,
        &ctx,
        ParseScope {
            in_layer: true,
            app_name: None,
        },
    )
        .ok_or_else(|| "invalid action".to_string())?;

    if let Some(err) = errors.first() {
        return Err(format!("{err}"));
    }

    Ok(action)
}
