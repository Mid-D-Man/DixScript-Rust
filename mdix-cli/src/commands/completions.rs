use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};
use crate::commands::GlobalOpts;

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Writes a completion script to stdout — e.g.
///   mdix completions zsh > ~/.zfunc/_mdix
///   mdix completions bash > /etc/bash_completion.d/mdix
pub fn run(args: CompletionsArgs, _global: &GlobalOpts) -> i32 {
    let mut cmd = crate::Cli::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut std::io::stdout());
    0
}
