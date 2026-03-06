use crate::cli::CompletionArgs;
use crate::cli::error::CliResult;
use clap::CommandFactory;
use clap_complete::generate;

pub fn run(args: CompletionArgs) -> CliResult<()> {
    let mut cmd = crate::cli::Cli::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
