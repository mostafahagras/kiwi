use crate::{
    config::{
        ValidationContext,
        error::ConfigError,
        utils::{KEY_SUGGESTIONS, MODIFIER_SUGGESTIONS, suggest_best_match},
    },
    key::{Key, KeyBinding, Modifiers},
};
use miette::SourceSpan;

pub fn parse_keybinding(
    raw_key: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<KeyBinding> {
    // let mode = if raw_key.starts_with('@') {
    //     KeyBindingMode::Logical
    // } else {
    //     KeyBindingMode::Physical
    // };
    let raw_key = raw_key.trim_start_matches('@');
    let parts: Vec<&str> = raw_key
        .split(|c: char| c == '+' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return None;
    }

    let mut corrected_parts = Vec::new();
    let mut has_typo = false;
    let mut first_typo = None;

    let mut resolved_mods = Modifiers::NONE;
    let mut resolved_key = None;
    let mut used_any_alias = false;

    // 1. Process each part
    for &part in &parts {
        let raw_m = Modifiers::parse(part);
        let is_valid_mod = !raw_m.is_empty();
        let is_valid_alias = ctx.modifier_names.contains(&part.to_string());
        let k = Key::parse(part);
        let is_valid_key = k.is_some();

        if is_valid_mod || is_valid_alias || is_valid_key {
            corrected_parts.push(part.to_string());

            if is_valid_mod {
                resolved_mods |= raw_m;
            } else if is_valid_alias {
                for (alias_mask, (name, _)) in ctx.modifier_map {
                    if name == part {
                        resolved_mods |= *alias_mask;
                        used_any_alias = true;
                        break;
                    }
                }
            } else if let Some(key) = k {
                resolved_key = Some(key);
            }
        } else if let Some(suggestion) = {
            let static_mods = MODIFIER_SUGGESTIONS.iter().copied();
            let user_mods = ctx.modifier_names.iter().map(|s| s.as_str());
            let pool = static_mods
                .chain(KEY_SUGGESTIONS.iter().copied())
                .chain(user_mods);
            suggest_best_match(part, pool)
        } {
            corrected_parts.push(suggestion);
            has_typo = true;
            if first_typo.is_none() {
                first_typo = Some(part.to_string());
            }
        } else {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw_key.to_string(),
                span,
                message: format!("Unrecognized key or modifier: '{}'", part),
            });
            return None;
        }
    }

    // 2. Report Typo Warnings
    if has_typo {
        errors.push(ConfigError::BindingTypo {
            src: ctx.src.clone(),
            raw: raw_key.to_string(),
            span,
            typo: first_typo.unwrap(),
            suggestion: format!("Did you mean '{}'?", corrected_parts.join("+")),
        });
    }

    // 3. Report Unoptimized Warnings
    if !used_any_alias
        && resolved_mods.bits().count_ones() > 1
        && let Some((alias_name, _)) = ctx.modifier_map.get(&resolved_mods)
    {
        errors.push(ConfigError::UnoptimizedBind {
            src: ctx.src.clone(),
            alias: alias_name.clone(),
            span,
        });
    }

    // 4. Construction
    match resolved_key {
        Some(key) => Some(KeyBinding {
            modifiers: resolved_mods,
            // mode,
            key,
        }),
        None => {
            errors.push(ConfigError::InvalidBinding {
                src: ctx.src.clone(),
                raw: raw_key.to_string(),
                span,
                message: "No base key found (e.g. 'a', 'esc', 'space')".into(),
            });
            None
        }
    }
}
