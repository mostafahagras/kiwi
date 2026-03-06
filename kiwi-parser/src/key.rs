use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Space,
    Backspace,
    Tab,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    VolumeUp,
    VolumeDown,
    Mute,
    BrightnessUp,
    BrightnessDown,
    KeyboardBrightnessUp,
    KeyboardBrightnessDown,
    PlayPause,
    NextTrack,
    PrevTrack,
    MissionControl,
    Spotlight,
    Dictation,
    DoNotDisturb,
}

impl Key {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.to_lowercase();
        match s.as_str() {
            "esc" | "escape" => Some(Self::Esc),
            "enter" | "return" => Some(Self::Enter),
            "space" | "spc" => Some(Self::Space),
            "backspace" | "bspc" => Some(Self::Backspace),
            "tab" => Some(Self::Tab),
            "up" => Some(Self::ArrowUp),
            "down" => Some(Self::ArrowDown),
            "left" => Some(Self::ArrowLeft),
            "right" => Some(Self::ArrowRight),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "pageup" | "pgup" => Some(Self::PageUp),
            "pagedown" | "pgdn" => Some(Self::PageDown),
            "del" | "delete" => Some(Self::Delete),
            "volumeup" | "volu" => Some(Self::VolumeUp),
            "volumedown" | "vold" => Some(Self::VolumeDown),
            "mute" => Some(Self::Mute),
            "brightnessup" | "brup" => Some(Self::BrightnessUp),
            "brightnessdown" | "brdn" => Some(Self::BrightnessDown),
            "keyboardbrightnessup" | "kbdbrightnessup" | "kbrup" | "kbup" => {
                Some(Self::KeyboardBrightnessUp)
            }
            "keyboardbrightnessdown" | "kbdbrightnessdown" | "kbrdn" | "kbdown" | "kbdn" => {
                Some(Self::KeyboardBrightnessDown)
            }
            "playpause" | "play" => Some(Self::PlayPause),
            "next" | "nexttrack" => Some(Self::NextTrack),
            "prev" | "prevtrack" => Some(Self::PrevTrack),
            "missioncontrol" | "mctl" => Some(Self::MissionControl),
            "spotlight" | "sl" | "sls" => Some(Self::Spotlight),
            "dictation" | "dict" | "dtn" => Some(Self::Dictation),
            "donotdisturb" | "dnd" => Some(Self::DoNotDisturb),
            _ if s.starts_with('f') && s.len() > 1 => s[1..].parse::<u8>().ok().map(Self::F),
            _ if s.chars().count() == 1 => s.chars().next().map(Self::Char),
            _ => None,
        }
    }

    pub fn is_media_key(&self) -> bool {
        matches!(
            self,
            Self::VolumeUp
                | Self::VolumeDown
                | Self::Mute
                | Self::BrightnessUp
                | Self::BrightnessDown
                | Self::KeyboardBrightnessUp
                | Self::KeyboardBrightnessDown
                | Self::PlayPause
                | Self::NextTrack
                | Self::PrevTrack
                | Self::MissionControl
                | Self::Spotlight
                | Self::Dictation
                | Self::DoNotDisturb
        )
    }

    pub fn is_non_interceptable_trigger_key(&self) -> bool {
        matches!(
            self,
            Self::MissionControl | Self::Spotlight | Self::Dictation | Self::DoNotDisturb
        )
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Char(c) => write!(f, "{}", c),
            Self::Space => write!(f, "space"),
            Self::Enter => write!(f, "enter"),
            Self::Esc => write!(f, "esc"),
            Self::Tab => write!(f, "tab"),
            Self::ArrowUp => write!(f, "up"),
            Self::ArrowDown => write!(f, "down"),
            Self::ArrowLeft => write!(f, "left"),
            Self::ArrowRight => write!(f, "right"),
            Self::F(n) => write!(f, "f{}", n),
            Self::Home => write!(f, "home"),
            Self::End => write!(f, "end"),
            Self::PageUp => write!(f, "pageup"),
            Self::PageDown => write!(f, "pagedown"),
            Self::Delete => write!(f, "delete"),
            Self::VolumeUp => write!(f, "volumeup"),
            Self::VolumeDown => write!(f, "volumedown"),
            Self::Mute => write!(f, "mute"),
            Self::BrightnessUp => write!(f, "brightnessup"),
            Self::BrightnessDown => write!(f, "brightnessdown"),
            Self::KeyboardBrightnessUp => write!(f, "keyboardbrightnessup"),
            Self::KeyboardBrightnessDown => write!(f, "keyboardbrightnessdown"),
            Self::PlayPause => write!(f, "playpause"),
            Self::NextTrack => write!(f, "nexttrack"),
            Self::PrevTrack => write!(f, "prevtrack"),
            Self::Backspace => write!(f, "backspace"),
            Self::MissionControl => write!(f, "missioncontrol"),
            Self::Spotlight => write!(f, "spotlight"),
            Self::Dictation => write!(f, "dictation"),
            Self::DoNotDisturb => write!(f, "donotdisturb"),
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    pub struct Modifiers: u8 {
        const NONE    = 0b0000;
        const CONTROL = 0b0001;
        const SHIFT   = 0b0010;
        const OPTION  = 0b0100;
        const COMMAND = 0b1000;
    }
}

impl Modifiers {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().trim().trim_matches('+') {
            "control" | "ctrl" | "ctl" => Self::CONTROL,
            "shift" | "sft" => Self::SHIFT,
            "option" | "opt" | "alt" | "alternative" => Self::OPTION,
            "command" | "cmd" | "meta" | "super" | "windows" | "win" => Self::COMMAND,
            _ => Self::NONE,
        }
    }
    pub fn from_parts(parts: Vec<&str>) -> Self {
        let mut modifiers = Modifiers::NONE;
        for part in parts {
            modifiers |= Modifiers::parse(part);
        }
        modifiers
    }
}

impl std::fmt::Display for Modifiers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.contains(Self::COMMAND) {
            parts.push("cmd");
        }
        if self.contains(Self::OPTION) {
            parts.push("opt");
        }
        if self.contains(Self::CONTROL) {
            parts.push("ctrl");
        }
        if self.contains(Self::SHIFT) {
            parts.push("shift");
        }

        if parts.is_empty() {
            write!(f, "") // Or "none" if you prefer visibility
        } else {
            write!(f, "{}", parts.join("+"))
        }
    }
}

// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
// pub enum KeyBindingMode {
//     #[default]
//     Physical,
//     Logical,
// }

#[derive(Eq, PartialEq, Hash, Clone)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: Key,
    // pub mode: KeyBindingMode,
}

impl std::fmt::Debug for KeyBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mods = self.modifiers.to_string();
        // let mode = match self.mode {
        //     KeyBindingMode::Physical => "",
        //     KeyBindingMode::Logical => "@",
        // };
        if mods.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", self.modifiers, self.key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn parses_keyboard_brightness_aliases() {
        assert!(matches!(
            Key::parse("kbrup"),
            Some(Key::KeyboardBrightnessUp)
        ));
        assert!(matches!(
            Key::parse("kbup"),
            Some(Key::KeyboardBrightnessUp)
        ));
        assert!(matches!(
            Key::parse("kbrdn"),
            Some(Key::KeyboardBrightnessDown)
        ));
        assert!(matches!(
            Key::parse("kbdown"),
            Some(Key::KeyboardBrightnessDown)
        ));
    }
}
