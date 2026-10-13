//! Entry point of Bracketlint.

use std::process::ExitCode;

use bracketlint::{ExitStatus, cli::Cli, run};
use clap::Parser;

// Parsing and formatting make many small allocations, which mimalloc serves
// faster than the system allocators do.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub fn main() -> ExitCode {
    // Enabled ANSI colours on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    // An argument that starts with `@` names a file of arguments, which may not
    // exist.
    let args = wild::args_os();
    let args = match argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: couldn't read the arguments in a file: {e}");
            return ExitStatus::Error.into();
        }
    };

    let args = Cli::parse_from(args);

    match run(args) {
        Ok(status) => status.into(),
        // An error that stops the run, rather than one in a template, is an
        // internal error. It is printed with its causes, e.g. why a
        // `bracketlint.toml` isn't valid.
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitStatus::Error.into()
        }
    }
}
