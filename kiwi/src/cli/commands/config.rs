use crate::cli::error::{CliError, CliResult};
use crate::cli::{ConfigArgs, ConfigCommand, ConfigInitArgs};
use std::fs;
use std::path::PathBuf;

const DEFAULT_CONFIG: &str = r#"[mods]
hyper = ["command", "option", "shift", "control"]

[binds]
"hyper+r" = "reload"
"#;

pub fn run(args: ConfigArgs) -> CliResult<()> {
    match args.command {
        ConfigCommand::Path => run_path(),
        ConfigCommand::Init(init_args) => run_init(init_args),
    }
}

fn run_path() -> CliResult<()> {
    let path = crate::resolve_config_path(None)
        .map_err(|e| CliError::new(format!("config path resolution failed: {e}")))?;
    println!("{}", path.display());
    Ok(())
}

fn run_init(args: ConfigInitArgs) -> CliResult<()> {
    let path = default_config_path()
        .map_err(|e| CliError::new(format!("failed to resolve default config path: {e}")))?;

    if path.exists() && !args.force {
        return Err(CliError::new(format!(
            "config already exists at {} (use --force to overwrite)",
            path.display()
        )));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CliError::new(format!(
                "failed to create config directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    fs::write(&path, DEFAULT_CONFIG).map_err(|e| {
        CliError::new(format!(
            "failed to write default config at {}: {e}",
            path.display()
        ))
    })?;

    println!("{}", path.display());
    Ok(())
}

fn default_config_path() -> Result<PathBuf, std::io::Error> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home).join(".kiwi").join("config.toml"))
}
