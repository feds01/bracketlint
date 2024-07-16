//! Entry point of Bracketlint.

use std::process::ExitCode;

use bracketlint::{cli::Cli, run};
use clap::Parser;

pub fn main() -> ExitCode {
    // Enabled ANSI colours on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    let args = wild::args_os();
    let args = argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX).unwrap();

    let args = Cli::parse_from(args);

    match run(args) {
        Ok(status) => status.into(),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
