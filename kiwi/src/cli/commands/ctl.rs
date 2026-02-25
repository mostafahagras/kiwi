use crate::cli::error::{CliError, CliResult};
use crate::cli::{BindingsArgs, CtlArgs, CtlCommand, LayerCommand};
use crate::control::{ControlRequest, ControlResponse, send_control_request};
use serde_json::to_string_pretty;

pub fn run(args: CtlArgs) -> CliResult<()> {
    let request = to_request(args.command)?;
    let response = send_control_request(args.socket, &request)?;

    if args.json {
        let payload = to_string_pretty(&response)
            .map_err(|e| CliError::new(format!("failed to render json response: {e}")))?;
        println!("{payload}");
    } else {
        print_human(&request, &response)?;
    }

    if response.ok {
        Ok(())
    } else {
        Err(CliError::new(
            response
                .message
                .unwrap_or_else(|| "control request failed".to_string()),
        ))
    }
}

fn to_request(command: CtlCommand) -> CliResult<ControlRequest> {
    match command {
        CtlCommand::Ping => Ok(ControlRequest::Ping),
        CtlCommand::Status => Ok(ControlRequest::Status),
        CtlCommand::Reload => Ok(ControlRequest::Reload),
        CtlCommand::Quit => Ok(ControlRequest::Quit),
        CtlCommand::ConfigPath => Ok(ControlRequest::ConfigPath),
        CtlCommand::Version => Ok(ControlRequest::Version),
        CtlCommand::Layer(layer) => match layer.command {
            LayerCommand::List => Ok(ControlRequest::LayerList),
            LayerCommand::Active => Ok(ControlRequest::LayerActive),
            LayerCommand::Activate { layer } => Ok(ControlRequest::LayerActivate { layer }),
        },
        CtlCommand::Send(BindingsArgs { bindings }) => {
            if bindings.is_empty() {
                return Err(CliError::new("ctl send requires at least one binding"));
            }
            Ok(ControlRequest::Send { bindings })
        }
        CtlCommand::Press(BindingsArgs { bindings }) => {
            if bindings.is_empty() {
                return Err(CliError::new("ctl press requires at least one binding"));
            }
            Ok(ControlRequest::Press { bindings })
        }
        CtlCommand::Exec(exec) => Ok(ControlRequest::Exec {
            action: exec.action,
        }),
    }
}

fn print_human(request: &ControlRequest, response: &ControlResponse) -> CliResult<()> {
    if !response.ok {
        return Err(CliError::new(
            response
                .message
                .clone()
                .unwrap_or_else(|| "control request failed".to_string()),
        ));
    }

    let Some(data) = response.data.as_ref() else {
        return Ok(());
    };

    match request {
        ControlRequest::Ping => println!("ok"),
        ControlRequest::Status => {
            if let Some(pid) = data.get("pid") {
                println!("pid: {pid}");
            }
            if let Some(version) = data.get("version") {
                println!("version: {version}");
            }
            if let Some(config) = data.get("config_path") {
                println!("config: {config}");
            }
            if let Some(uptime) = data.get("uptime_secs") {
                println!("uptime_secs: {uptime}");
            }
            if let Some(intercept) = data.get("intercept_mode") {
                println!("intercept: {intercept}");
            }
            if let Some(depth) = data.get("queue_depth") {
                println!("queue_depth: {depth}");
            }
            if let Some(active_layers) = data.get("active_layers") {
                println!("active_layers: {active_layers}");
            }
        }
        ControlRequest::Reload => println!("reloaded"),
        ControlRequest::Quit => println!("quitting"),
        ControlRequest::ConfigPath => {
            if let Some(path) = data.get("config_path") {
                println!("{}", path.as_str().unwrap_or_default());
            }
        }
        ControlRequest::Version => {
            if let Some(version) = data.get("version") {
                println!("{}", version.as_str().unwrap_or_default());
            }
        }
        ControlRequest::LayerList => {
            if let Some(layers) = data.get("layers").and_then(|v| v.as_array()) {
                for layer in layers {
                    if let Some(name) = layer.as_str() {
                        println!("{name}");
                    }
                }
            }
        }
        ControlRequest::LayerActive => {
            if let Some(layers) = data.get("active_layers").and_then(|v| v.as_array()) {
                for layer in layers {
                    if let Some(name) = layer.as_str() {
                        println!("{name}");
                    }
                }
            }
        }
        ControlRequest::LayerActivate { .. } => {
            if let Some(name) = data.get("activated").and_then(|v| v.as_str()) {
                println!("activated {name}");
            }
        }
        ControlRequest::Send { .. } | ControlRequest::Press { .. } => {
            let processed = data
                .get("processed")
                .and_then(|v| v.as_u64())
                .unwrap_or_default();
            let failed = data
                .get("failed")
                .and_then(|v| v.as_u64())
                .unwrap_or_default();
            println!("processed={processed} failed={failed}");
        }
        ControlRequest::Exec { .. } => {
            if let Some(kind) = data.get("action_kind").and_then(|v| v.as_str()) {
                println!("executed {kind}");
            }
        }
    }

    Ok(())
}
