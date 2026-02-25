use crate::cli::error::CliError;
use crate::hotkey::HotkeyManager;
use crate::manager::{
    self, InterceptState, action_queue_depth, clear_window_state, dispatch_action, intercept_state,
};
use kiwi_parser::{Action, Key, KeyBinding, Modifiers, parse_action_str};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, error};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ControlRequest {
    Ping,
    Status,
    Reload,
    Quit,
    ConfigPath,
    Version,
    LayerList,
    LayerActive,
    LayerActivate { layer: String },
    Send { bindings: Vec<String> },
    Press { bindings: Vec<String> },
    Exec { action: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ControlResponse {
    pub ok: bool,
    pub code: Option<String>,
    pub message: Option<String>,
    pub data: Option<Value>,
}

impl ControlResponse {
    fn ok(data: Value) -> Self {
        Self {
            ok: true,
            code: None,
            message: None,
            data: Some(data),
        }
    }

    fn err(code: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            code: Some(code.to_string()),
            message: Some(message.into()),
            data: None,
        }
    }
}

pub struct ControlState {
    pub manager: Arc<Mutex<HotkeyManager>>,
    pub config_path: PathBuf,
    pub started_at: Instant,
    pub socket_path: PathBuf,
}

pub fn default_socket_path() -> Result<PathBuf, CliError> {
    let home = std::env::var("HOME")
        .map_err(|_| CliError::new("HOME not set; cannot resolve control socket path"))?;
    Ok(PathBuf::from(home)
        .join(".kiwi")
        .join("kiwi.sock"))
}

pub fn spawn_control_server(state: ControlState) -> Result<(), CliError> {
    if let Some(parent) = state.socket_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CliError::new(format!(
                "failed to create control directory {}: {e}",
                parent.display()
            ))
        })?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| {
            CliError::new(format!(
                "failed to set control directory permissions {}: {e}",
                parent.display()
            ))
        })?;
    }

    if state.socket_path.exists() {
        fs::remove_file(&state.socket_path).map_err(|e| {
            CliError::new(format!(
                "failed to remove stale control socket {}: {e}",
                state.socket_path.display()
            ))
        })?;
    }

    let listener = UnixListener::bind(&state.socket_path).map_err(|e| {
        CliError::new(format!(
            "failed to bind control socket {}: {e}",
            state.socket_path.display()
        ))
    })?;

    fs::set_permissions(&state.socket_path, fs::Permissions::from_mode(0o600)).map_err(|e| {
        CliError::new(format!(
            "failed to set control socket permissions {}: {e}",
            state.socket_path.display()
        ))
    })?;

    std::thread::Builder::new()
        .name("kiwi-control-server".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Err(err) = handle_client(stream, &state) {
                            error!("control client error: {err}");
                        }
                    }
                    Err(err) => error!("control server accept error: {err}"),
                }
            }
        })
        .map_err(|e| CliError::new(format!("failed to start control server thread: {e}")))?;

    Ok(())
}

fn handle_client(mut stream: UnixStream, state: &ControlState) -> Result<(), String> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader
            .read_line(&mut line)
            .map_err(|e| format!("failed reading request: {e}"))?;
    }

    let request: ControlRequest =
        serde_json::from_str(&line).map_err(|e| format!("invalid request json: {e}"))?;

    let response = handle_request(request, state);
    let payload = serde_json::to_string(&response).map_err(|e| format!("encode response: {e}"))?;

    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("failed writing response: {e}"))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("failed writing response newline: {e}"))?;

    Ok(())
}

fn handle_request(request: ControlRequest, state: &ControlState) -> ControlResponse {
    match request {
        ControlRequest::Ping => ControlResponse::ok(json!({"pong": true})),
        ControlRequest::Status => {
            let (layers, intercept) = match state.manager.lock() {
                Ok(mgr) => (mgr.active_layer_names(), intercept_state_label(intercept_state())),
                Err(_) => {
                    return ControlResponse::err("internal", "failed to lock hotkey manager");
                }
            };

            ControlResponse::ok(json!({
                "alive": true,
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
                "config_path": state.config_path,
                "uptime_secs": state.started_at.elapsed().as_secs(),
                "active_layers": layers,
                "intercept_mode": intercept,
                "queue_depth": action_queue_depth(),
            }))
        }
        ControlRequest::Reload => match crate::parse_config_from_path(&state.config_path) {
            Ok(config) => match state.manager.lock() {
                Ok(mut mgr) => {
                    *mgr = manager::setup_manager(&config);
                    clear_window_state();
                    ControlResponse::ok(json!({"reloaded": true}))
                }
                Err(_) => ControlResponse::err("internal", "failed to lock hotkey manager"),
            },
            Err(err) => ControlResponse::err("reload_failed", format!("{err:?}")),
        },
        ControlRequest::Quit => {
            dispatch_action(Action::Quit);
            ControlResponse::ok(json!({"quitting": true}))
        }
        ControlRequest::ConfigPath => {
            ControlResponse::ok(json!({"config_path": state.config_path.to_string_lossy()}))
        }
        ControlRequest::Version => {
            ControlResponse::ok(json!({"version": env!("CARGO_PKG_VERSION")}))
        }
        ControlRequest::LayerList => match state.manager.lock() {
            Ok(mgr) => ControlResponse::ok(json!({"layers": mgr.registered_layer_names()})),
            Err(_) => ControlResponse::err("internal", "failed to lock hotkey manager"),
        },
        ControlRequest::LayerActive => match state.manager.lock() {
            Ok(mgr) => ControlResponse::ok(json!({"active_layers": mgr.active_layer_names()})),
            Err(_) => ControlResponse::err("internal", "failed to lock hotkey manager"),
        },
        ControlRequest::LayerActivate { layer } => match state.manager.lock() {
            Ok(mut mgr) => {
                if layer.eq_ignore_ascii_case("root") {
                    mgr.clear_active_layers();
                    return ControlResponse::ok(json!({"activated": "root"}));
                }

                let app = crate::window::get_focused_app();
                match mgr.activate_layer(&layer, &app) {
                    Ok(_) => ControlResponse::ok(json!({"activated": layer})),
                    Err(msg) => ControlResponse::err("unknown_layer", msg),
                }
            }
            Err(_) => ControlResponse::err("internal", "failed to lock hotkey manager"),
        },
        ControlRequest::Send { bindings } => {
            let mut processed = 0usize;
            for raw in bindings {
                let binding = match parse_binding(&raw) {
                    Ok(binding) => binding,
                    Err(msg) => {
                        return ControlResponse::err("invalid_binding", msg);
                    }
                };

                let app = crate::window::get_focused_app();
                match state.manager.lock() {
                    Ok(mut mgr) => {
                        let outcome = manager::process_injected_binding(&mut mgr, &binding, &app);
                        processed += 1;
                        debug!(
                            "ctl send injected binding {} handled={} dispatched={}",
                            raw, outcome.handled, outcome.dispatched_actions
                        );
                    }
                    Err(_) => {
                        return ControlResponse::err("internal", "failed to lock hotkey manager");
                    }
                }
            }

            ControlResponse::ok(json!({"processed": processed, "failed": 0}))
        }
        ControlRequest::Press { bindings } => {
            let mut processed = 0usize;
            for raw in bindings {
                let binding = match parse_binding(&raw) {
                    Ok(binding) => binding,
                    Err(msg) => {
                        return ControlResponse::err("invalid_binding", msg);
                    }
                };

                crate::input::send_key_combination(&binding);
                processed += 1;
            }
            ControlResponse::ok(json!({"processed": processed, "failed": 0}))
        }
        ControlRequest::Exec { action } => {
            let action = match parse_action_str(&action) {
                Ok(action) => action,
                Err(msg) => return ControlResponse::err("invalid_action", msg),
            };

            if contains_shell(&action) {
                return ControlResponse::err("forbidden_action", "shell actions are not allowed");
            }

            let kind = action_kind(&action);
            dispatch_action(action);
            ControlResponse::ok(json!({"action_kind": kind, "accepted": true}))
        }
    }
}

pub fn send_control_request(
    socket: Option<PathBuf>,
    request: &ControlRequest,
) -> Result<ControlResponse, CliError> {
    let socket = socket.unwrap_or(default_socket_path()?);
    let mut stream = UnixStream::connect(&socket).map_err(|e| {
        CliError::new(format!(
            "daemon not running or socket unavailable at {}: {e}",
            socket.display()
        ))
    })?;

    let payload = serde_json::to_string(request)
        .map_err(|e| CliError::new(format!("failed to encode request: {e}")))?;
    stream
        .write_all(payload.as_bytes())
        .map_err(|e| CliError::new(format!("failed to send request: {e}")))?;
    stream
        .write_all(b"\n")
        .map_err(|e| CliError::new(format!("failed to finalize request: {e}")))?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .map_err(|e| CliError::new(format!("failed to read response: {e}")))?;

    serde_json::from_str(&line)
        .map_err(|e| CliError::new(format!("failed to decode response: {e}")))
}

fn parse_binding(raw: &str) -> Result<KeyBinding, String> {
    let parts: Vec<&str> = raw
        .split(|c: char| c == '+' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();

    if parts.is_empty() {
        return Err("binding is empty".to_string());
    }

    let mut mods = Modifiers::NONE;
    let mut key: Option<Key> = None;

    for part in parts {
        let parsed_mod = Modifiers::parse(part);
        if !parsed_mod.is_empty() {
            mods |= parsed_mod;
            continue;
        }

        if let Some(parsed_key) = Key::parse(part) {
            if key.replace(parsed_key).is_some() {
                return Err(format!("binding '{raw}' contains multiple keys"));
            }
            continue;
        }

        return Err(format!("invalid key or modifier in binding: '{part}'"));
    }

    let key = key.ok_or_else(|| format!("binding '{raw}' does not contain a key"))?;
    Ok(KeyBinding {
        modifiers: mods,
        key,
    })
}

fn contains_shell(action: &Action) -> bool {
    match action {
        Action::Shell(_) => true,
        Action::Sequence(actions) => actions.iter().any(contains_shell),
        _ => false,
    }
}

fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::Shell(_) => "shell",
        Action::Remap(_) => "remap",
        Action::Snap(_) => "snap",
        Action::Resize(_) => "resize",
        Action::Reload => "reload",
        Action::Quit => "quit",
        Action::SleepFor(_) => "sleep_for",
        Action::SleepUntil(_) => "sleep_until",
        Action::Swallow(_) => "swallow",
        Action::Pass(_) => "pass",
        Action::Sequence(_) => "sequence",
    }
}

fn intercept_state_label(state: InterceptState) -> &'static str {
    match state {
        InterceptState::None => "none",
        InterceptState::Pass => "pass",
        InterceptState::Swallow => "swallow",
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_shell, parse_binding};
    use kiwi_parser::{Action, Key, KeyBinding, Modifiers, parse_action_str};

    #[test]
    fn parse_binding_accepts_modifier_and_key() {
        let binding = parse_binding("cmd+k").expect("binding should parse");
        assert_eq!(
            binding,
            KeyBinding {
                modifiers: Modifiers::COMMAND,
                key: Key::Char('k'),
            }
        );
    }

    #[test]
    fn parse_binding_rejects_multiple_keys() {
        let err = parse_binding("k+l").expect_err("binding should fail");
        assert!(err.contains("multiple keys"));
    }

    #[test]
    fn exec_policy_rejects_shell_action() {
        let action = parse_action_str("shell:echo hi").expect("action parse should succeed");
        assert!(contains_shell(&action));
    }

    #[test]
    fn exec_policy_rejects_shell_in_sequence() {
        let action = Action::Sequence(vec![
            parse_action_str("snap:LeftHalf").expect("snap parse should succeed"),
            parse_action_str("shell:echo hi").expect("shell parse should succeed"),
        ]);
        assert!(contains_shell(&action));
    }
}
