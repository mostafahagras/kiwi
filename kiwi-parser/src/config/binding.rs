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
    parse_keybinding_inner(raw_key, span, errors, ctx, false)
}

pub fn parse_remap_keybinding(
    raw_key: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
) -> Option<KeyBinding> {
    parse_keybinding_inner(raw_key, span, errors, ctx, true)
}

fn parse_keybinding_inner(
    raw_key: &str,
    span: SourceSpan,
    errors: &mut Vec<ConfigError>,
    ctx: &ValidationContext,
    allow_media_key: bool,
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
        Some(key) => {
            if !allow_media_key && key.is_non_interceptable_trigger_key() {
                errors.push(ConfigError::InvalidBinding {
                    src: ctx.src.clone(),
                    raw: raw_key.to_string(),
                    span,
                    message: "This key is not interceptable as a trigger binding (allowed only as remap target)".into(),
                });
                return None;
            }

            Some(KeyBinding {
                modifiers: resolved_mods,
                // mode,
                key,
            })
        }
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

#[cfg(test)]
mod tests {
    use super::{parse_keybinding, parse_remap_keybinding};
    use crate::config::ValidationContext;
    use crate::key::{Key, Modifiers};
    use miette::{NamedSource, SourceSpan};
    use std::collections::HashMap;

    fn ctx<'a>(
        src: &'a NamedSource<String>,
        modifier_map: &'a HashMap<Modifiers, (String, SourceSpan)>,
    ) -> ValidationContext<'a> {
        ValidationContext {
            src,
            modifier_map,
            modifier_names: Vec::new(),
            app_aliases: HashMap::new(),
        }
    }

    #[test]
    fn trigger_binding_rejects_non_interceptable_special_keys() {
        let src = NamedSource::new("test.toml", "".to_string());
        let modifier_map: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();
        let context = ctx(&src, &modifier_map);
        let mut errors = Vec::new();

        let parsed = parse_keybinding(
            "missioncontrol",
            SourceSpan::new(0.into(), 14),
            &mut errors,
            &context,
        );
        assert!(parsed.is_none());
        assert!(!errors.is_empty());
    }

    #[test]
    fn trigger_binding_allows_interceptable_media_keys() {
        let src = NamedSource::new("test.toml", "".to_string());
        let modifier_map: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();
        let context = ctx(&src, &modifier_map);
        let mut errors = Vec::new();

        let parsed = parse_keybinding(
            "volumeup",
            SourceSpan::new(0.into(), 8),
            &mut errors,
            &context,
        )
        .expect("trigger keybinding should parse");

        assert_eq!(parsed.key, Key::VolumeUp);
        assert!(errors.is_empty());
    }

    #[test]
    fn remap_binding_allows_media_keys_with_modifiers() {
        let src = NamedSource::new("test.toml", "".to_string());
        let modifier_map: HashMap<Modifiers, (String, SourceSpan)> = HashMap::new();
        let context = ctx(&src, &modifier_map);
        let mut errors = Vec::new();

        let parsed = parse_remap_keybinding(
            "cmd+shift+volumeup",
            SourceSpan::new(0.into(), 18),
            &mut errors,
            &context,
        )
        .expect("remap keybinding should parse");

        assert_eq!(parsed.key, Key::VolumeUp);
        assert!(parsed.modifiers.contains(Modifiers::COMMAND));
        assert!(parsed.modifiers.contains(Modifiers::SHIFT));
        assert!(errors.is_empty());
    }
}
