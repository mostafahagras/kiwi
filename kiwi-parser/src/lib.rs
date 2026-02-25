mod config;
mod key;
mod layout;

pub use config::action::{Action, Resize, Snap, parse_action_str};
pub use config::layer::{Layer, LayerMode};
pub use config::{Config, parse_config};
pub use key::{Key, KeyBinding, Modifiers};
