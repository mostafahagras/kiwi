use crate::cli::UpdateArgs;
use crate::cli::commands::install;
use crate::cli::error::{CliError, CliResult};
use std::path::Path;
use std::process::Command;

pub fn run(args: UpdateArgs) -> CliResult<()> {
    let repo_dir = match args.path {
        Some(path) => path,
        None => std::env::current_dir()
            .map_err(|e| CliError::new(format!("failed to detect current directory: {e}")))?,
    };

    ensure_repo_root(&repo_dir)?;

    let status = Command::new("cargo")
        .args(["install", "--path", "kiwi", "--force"])
        .current_dir(&repo_dir)
        .status()
        .map_err(|e| CliError::new(format!("failed to execute cargo install: {e}")))?;

    if !status.success() {
        return Err(CliError::with_code(
            1,
            "cargo install failed; run from kiwi repo root or pass --path",
        ));
    }

    // Reinstall app bundle / LaunchAgent so the managed daemon uses the updated binary.
    install::run().map_err(|e| CliError::new(format!("install failed after update: {e}")))
}

fn ensure_repo_root(repo_dir: &Path) -> CliResult<()> {
    let cargo_toml = repo_dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(CliError::new(format!(
            "{} is not a workspace root (missing Cargo.toml)",
            repo_dir.display()
        )));
    }

    Ok(())
}
