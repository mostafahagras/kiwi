use crate::cli::CheckArgs;
use crate::cli::error::{CliError, CliResult};

pub fn run(args: CheckArgs) -> CliResult<()> {
    let path = crate::resolve_config_path(args.config)
        .map_err(|e| CliError::new(format!("config path resolution failed: {e}")))?;

    match crate::parse_config_from_path(&path) {
        Ok(_) => Ok(()),
        Err(report) => {
            eprintln!("{report:?}");
            Err(CliError::silent(1))
        }
    }
}
