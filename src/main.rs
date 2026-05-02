use std::process::ExitCode;

use clap::Parser;
use ctxc::cli::{run, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    ExitCode::from(run(cli) as u8)
}
