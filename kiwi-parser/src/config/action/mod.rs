pub mod resize;
pub mod snap;

use crate::config::{ValidationContext, binding::parse_keybinding, error::ConfigError};
use crate::key::KeyBinding;
use miette::SourceSpan;
pub use resize::Resize;
pub use snap::Snap;
use std::time::{Duration, Instant};
use toml_span::value::ValueInner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Executes any shell command.
    /// Use with caution
    Shell(String),
    /// Remaps a keybinding to another
    Remap(KeyBinding),
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
}

pub fn parse_action(
    value: &toml_span::Value,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<Action> {
    let span = SourceSpan::new(
        value.span.start.into(),
        value.span.end - value.span.start,
    );

    match value.as_ref() {
        // Case A: A single action string (e.g., "snap:left")
        ValueInner::String(raw_value) => parse_single_action_string(raw_value, span, errors, ctx),

        // Case B: A list of actions (e.g., ["shell:say hi", "sleep:500", "quit"])
        ValueInner::Array(arr) => {
            let mut actions = Vec::with_capacity(arr.len());
            for item in arr {
                if let Some(action) = parse_action(item, errors, ctx) {
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
) -> Option<Action> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((prefix, payload)) = trimmed.split_once(':') {
        let payload = payload.trim();
        match prefix {
            "shell" => Some(Action::Shell(payload.to_string())),
            "remap" => parse_keybinding(payload, span, errors, ctx).map(Action::Remap),
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
                    help: "Valid prefixes: shell, remap, snap, resize, sleep, swallow, pass".into(),
                });
                None
            }
        }
    } else {
        match trimmed {
            "reload" => Some(Action::Reload),
            "quit" => Some(Action::Quit),
            _ => Some(Action::Shell(trimmed.to_string())),
        }
    }
}
