pub const MODIFIER_SUGGESTIONS: &[&str] = &[
    // Control
    "control",
    "ctrl",
    "ctl",
    // Shift
    "shift",
    "sft",
    // Option
    "option",
    "opt",
    "alt",
    "alternative",
    // Command
    "command",
    "cmd",
    "meta",
    "super",
    "windows",
    "win",
];

pub const KEY_SUGGESTIONS: &[&str] = &[
    "esc",
    "escape",
    "enter",
    "return",
    "space",
    "tab",
    "backspace",
    "delete",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "volumeup",
    "volumedown",
    "mute",
    "brightnessup",
    "brightnessdown",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

pub fn suggest_best_match<I, T>(typo: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    candidates
        .into_iter()
        .map(|cand| {
            let s = cand.as_ref();
            (s.to_string(), strsim::levenshtein(typo, s))
        })
        // Distance of 2 is usually the sweet spot for typos
        .filter(|(_, dist)| *dist <= 2)
        .min_by_key(|(_, dist)| *dist)
        .map(|(cand, _)| cand)
}

// Simple helper for fuzzy matching
pub fn is_similar(a: &str, b: &str) -> bool {
    let distance = strsim::levenshtein(a, b);
    distance > 0 && distance <= 2 // Threshold for a "typo"
}
