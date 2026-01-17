use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapSide {
    Left,
    Right,
    Full,
    Top,
    Bottom,
    Fullscreen,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    // New variants
    Maximize,
    MaximizeHeight,
    MaximizeWidth,
    Center,
    Restore,
    ReasonableSize,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    FirstThird,
    CenterThird,
    LastThird,
    FirstTwoThirds,
    LastTwoThirds,
    FirstFourth,
    SecondFourth,
    ThirdFourth,
    LastFourth,
    TopLeftSixth,
    TopCenterSixth,
    TopRightSixth,
    BottomLeftSixth,
    BottomCenterSixth,
    BottomRightSixth,
}

impl FromStr for SnapSide {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "left" => Ok(SnapSide::Left),
            "right" => Ok(SnapSide::Right),
            "full" | "fullscreen" => Ok(SnapSide::Full),
            "top" | "up" => Ok(SnapSide::Top),
            "bottom" | "down" => Ok(SnapSide::Bottom),
            "topleft" => Ok(SnapSide::TopLeft),
            "topright" => Ok(SnapSide::TopRight),
            "bottomleft" => Ok(SnapSide::BottomLeft),
            "bottomright" => Ok(SnapSide::BottomRight),
            // New mappings
            "maximize" => Ok(SnapSide::Maximize),
            "maximizeheight" => Ok(SnapSide::MaximizeHeight),
            "maximizewidth" => Ok(SnapSide::MaximizeWidth),
            "center" => Ok(SnapSide::Center),
            "restore" => Ok(SnapSide::Restore),
            "reasonablesize" => Ok(SnapSide::ReasonableSize),
            "moveup" => Ok(SnapSide::MoveUp),
            "movedown" => Ok(SnapSide::MoveDown),
            "moveleft" => Ok(SnapSide::MoveLeft),
            "moveright" => Ok(SnapSide::MoveRight),
            "firstthird" => Ok(SnapSide::FirstThird),
            "centerthird" => Ok(SnapSide::CenterThird),
            "lastthird" => Ok(SnapSide::LastThird),
            "firsttwothirds" => Ok(SnapSide::FirstTwoThirds),
            "lasttwothirds" => Ok(SnapSide::LastTwoThirds),
            "firstfourth" => Ok(SnapSide::FirstFourth),
            "secondfourth" => Ok(SnapSide::SecondFourth),
            "thirdfourth" => Ok(SnapSide::ThirdFourth),
            "lastfourth" => Ok(SnapSide::LastFourth),
            "topleftsixth" => Ok(SnapSide::TopLeftSixth),
            "topcentersixth" => Ok(SnapSide::TopCenterSixth),
            "toprightsixth" => Ok(SnapSide::TopRightSixth),
            "bottomleftsixth" => Ok(SnapSide::BottomLeftSixth),
            "bottomcentersixth" => Ok(SnapSide::BottomCenterSixth),
            "bottomrightsixth" => Ok(SnapSide::BottomRightSixth),
            _ => Err(format!("Unknown snap side: {}", s)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Space,
    Backspace,
    Tab,
    Shift,
    Control,
    Alt,
    Command,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F(u8),
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    CapsLock,
    Function,
    VolumeUp,
    VolumeDown,
    Mute,
    Unknown(u16),
}

impl FromStr for Key {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "esc" | "escape" => Ok(Key::Esc),
            "enter" | "return" => Ok(Key::Enter),
            "space" => Ok(Key::Space),
            "backspace" => Ok(Key::Backspace),
            "tab" => Ok(Key::Tab),
            "shift" => Ok(Key::Shift),
            "ctrl" | "control" => Ok(Key::Control),
            "alt" | "option" => Ok(Key::Alt),
            "cmd" | "command" => Ok(Key::Command),
            "up" => Ok(Key::ArrowUp),
            "down" => Ok(Key::ArrowDown),
            "left" => Ok(Key::ArrowLeft),
            "right" => Ok(Key::ArrowRight),
            "home" => Ok(Key::Home),
            "end" => Ok(Key::End),
            "pageup" => Ok(Key::PageUp),
            "pagedown" => Ok(Key::PageDown),
            "delete" => Ok(Key::Delete),
            "capslock" => Ok(Key::CapsLock),
            "fn" | "function" => Ok(Key::Function),
            s if s.starts_with('f') && s.len() > 1 => {
                s[1..].parse().map(Key::F).map_err(|_| "Invalid F-key".into())
            }
            s if s.chars().count() == 1 => Ok(Key::Char(s.chars().next().unwrap())),
            _ => Err(format!("Unknown key: {}", s)),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub command: bool,
}

impl Modifiers {
    pub fn from_vec(mods: &[String], aliases: &HashMap<String, Vec<String>>) -> Self {
        let mut result = Modifiers::default();
        for m in mods {
            let m = m.trim();
            let alias_match = aliases.iter().find(|(k, _)| k.trim().eq_ignore_ascii_case(m));
            if let Some((_, alias_mods)) = alias_match {
                let resolved = Modifiers::from_vec(alias_mods, aliases);
                result.shift |= resolved.shift;
                result.control |= resolved.control;
                result.alt |= resolved.alt;
                result.command |= resolved.command;
            } else {
                match m.to_lowercase().as_str() {
                    "shift" => result.shift = true,
                    "ctrl" | "control" => result.control = true,
                    "alt" | "option" => result.alt = true,
                    "cmd" | "command" => result.command = true,
                    _ => {}
                }
            }
        }
        result
    }

    pub fn from_cg_flags(flags: core_graphics::event::CGEventFlags) -> Self {
        use core_graphics::event::CGEventFlags;
        Self {
            shift: flags.contains(CGEventFlags::CGEventFlagShift),
            control: flags.contains(CGEventFlags::CGEventFlagControl),
            alt: flags.contains(CGEventFlags::CGEventFlagAlternate),
            command: flags.contains(CGEventFlags::CGEventFlagCommand),
        }
    }
}

impl Key {
    pub fn from_cg_code(code: u16, char: Option<char>) -> Self {
        match code {
            0x00 => Key::Char('a'), 0x01 => Key::Char('s'), 0x02 => Key::Char('d'), 0x03 => Key::Char('f'),
            0x04 => Key::Char('h'), 0x05 => Key::Char('g'), 0x06 => Key::Char('z'), 0x07 => Key::Char('x'),
            0x08 => Key::Char('c'), 0x09 => Key::Char('v'), 0x0B => Key::Char('b'), 0x0C => Key::Char('q'),
            0x0D => Key::Char('w'), 0x0E => Key::Char('e'), 0x0F => Key::Char('r'), 0x10 => Key::Char('y'),
            0x11 => Key::Char('t'), 0x12 => Key::Char('1'), 0x13 => Key::Char('2'), 0x14 => Key::Char('3'),
            0x15 => Key::Char('4'), 0x16 => Key::Char('6'), 0x17 => Key::Char('5'), 0x18 => Key::Char('='),
            0x19 => Key::Char('9'), 0x1A => Key::Char('7'), 0x1B => Key::Char('-'), 0x1C => Key::Char('8'),
            0x1D => Key::Char('0'), 0x1E => Key::Char(']'), 0x1F => Key::Char('o'), 0x20 => Key::Char('u'),
            0x21 => Key::Char('['), 0x22 => Key::Char('i'), 0x23 => Key::Char('p'), 0x25 => Key::Char('l'),
            0x26 => Key::Char('j'), 0x27 => Key::Char('\''), 0x28 => Key::Char('k'), 0x29 => Key::Char(';'),
            0x2A => Key::Char('\\'), 0x2B => Key::Char(','), 0x2C => Key::Char('/'), 0x2D => Key::Char('n'),
            0x2E => Key::Char('m'), 0x2F => Key::Char('.'), 0x32 => Key::Char('`'),

            // Special Keys
            0x35 => Key::Esc,
            0x24 => Key::Enter,
            0x31 => Key::Space,
            0x33 => Key::Backspace,
            0x30 => Key::Tab,
            0x38 | 0x3C => Key::Shift,
            0x3B | 0x3E => Key::Control,
            0x3A | 0x3D => Key::Alt,
            0x37 | 0x36 => Key::Command,
            0x7E => Key::ArrowUp,
            0x7D => Key::ArrowDown,
            0x7B => Key::ArrowLeft,
            0x7C => Key::ArrowRight,
            0x7A => Key::F(1),
            0x78 => Key::F(2),
            0x63 => Key::F(3),
            0x76 => Key::F(4),
            0x60 => Key::F(5),
            0x61 => Key::F(6),
            0x62 => Key::F(7),
            0x64 => Key::F(8),
            0x65 => Key::F(9),
            0x6D => Key::F(10),
            0x67 => Key::F(11),
            0x6F => Key::F(12),
            0x69 => Key::F(13),
            0x6B => Key::F(14),
            0x71 => Key::F(15),
            0x6A => Key::F(16),
            0x40 => Key::F(17),
            0x4F => Key::F(18),
            0x50 => Key::F(19),
            0x5A => Key::F(20),
            0x73 => Key::Home,
            0x77 => Key::End,
            0x74 => Key::PageUp,
            0x79 => Key::PageDown,
            0x75 => Key::Delete,
            0x39 => Key::CapsLock,
            0x48 => Key::VolumeUp,
            0x49 => Key::VolumeDown,
            0x4A => Key::Mute,
            _ => match char {
                Some(mut c) => {
                    if (1..=26).contains(&(c as u32)) {
                        if let Some(base_char) = char::from_u32((c as u32 - 1) + 'a' as u32) {
                            c = base_char;
                        }
                    }
                    Key::Char(c.to_ascii_lowercase())
                }
                None => Key::Unknown(code),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct KeyCombination {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyCombination {
    pub fn from_str_with_context(s: &str, aliases: &HashMap<String, Vec<String>>) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();
        if parts.is_empty() {
            return Err("Empty key combination".into());
        }
        let key_str = parts.last().unwrap();
        let key = Key::from_str(key_str)?;
        
        let mod_parts: Vec<String> = parts[..parts.len() - 1]
            .iter()
            .map(|s| s.to_string())
            .collect();
            
        let modifiers = Modifiers::from_vec(&mod_parts, aliases);
        Ok(KeyCombination { modifiers, key })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    Open(String),
    Snap(SnapSide),
    Remap(String),
    Shell(String),
    Reload,
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Command {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "reload" {
            return Ok(Command::Reload);
        }
        if let Some(rest) = s.strip_prefix("open:") {
            return Ok(Command::Open(rest.to_string()));
        }
        if let Some(rest) = s.strip_prefix("snap:") {
            return Ok(Command::Snap(SnapSide::from_str(rest)?));
        }
        if let Some(rest) = s.strip_prefix("send:") {
            return Ok(Command::Remap(rest.to_string()));
        }
        if let Some(rest) = s.strip_prefix("remap:") {
            return Ok(Command::Remap(rest.to_string()));
        }
        if let Some(rest) = s.strip_prefix("shell:") {
            return Ok(Command::Shell(rest.to_string()));
        }
        Ok(Command::Shell(s.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LayerItem {
    Command(Command),
    Layer(Layer),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub activate: Option<String>,
    #[serde(flatten)]
    pub items: HashMap<String, LayerItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub items: HashMap<String, LayerItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub modifiers: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub apps: HashMap<String, String>,
    #[serde(default)]
    pub layer: HashMap<String, Layer>,
    #[serde(default)]
    pub app: HashMap<String, AppConfig>,
    #[serde(default)]
    pub binds: HashMap<String, Command>, // Support top-level binds just in case
}
