//! Library definition of `bl` crate.

#![feature(panic_payload_as_str)]

pub mod cli;
mod commands;
mod crash;
pub(crate) mod version;

use std::{
    panic,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Ok, Result};
use bl_lints::settings::FixMode;
use bl_utils::{logging::ToolLogger, stream::CompilerOutputStream};
use bl_workspace::settings::Settings;
use cli::CheckCommand;
use crash::crash_handler;

#[derive(Copy, Clone)]
pub enum ExitStatus {
    /// Linting was successful and there were no linting errors.
    Success,
    /// Linting was successful but there were linting errors.
    Failure,
    /// Linting failed.
    Error,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
        }
    }
}

pub static LOGGER: ToolLogger = ToolLogger::new();

/// Handler function which will delegate functionality to the appropriate
/// command.
pub fn run(cli::Cli { command }: cli::Cli) -> Result<ExitStatus> {
    // Initial grunt work, panic handler and logger setup...
    panic::set_hook(Box::new(crash_handler));

    let output_stream = CompilerOutputStream::stdout;
    let error_stream = CompilerOutputStream::stderr;

    log::set_logger(&LOGGER).unwrap_or_else(|_| panic!("couldn't initiate logger"));

    LOGGER.error_stream.set(error_stream()).unwrap();
    LOGGER.output_stream.set(output_stream()).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    // We also need to create a global-config

    match command {
        cli::Command::Check(args) => check(args),
        cli::Command::Version => version(),
    }
}

/// Returns the default set of files if none are provided, otherwise returns
/// `None`.
fn resolve_default_files(files: Vec<PathBuf>, is_stdin: bool) -> Vec<PathBuf> {
    if files.is_empty() {
        if is_stdin {
            vec![Path::new("-").to_path_buf()]
        } else {
            vec![Path::new(".").to_path_buf()]
        }
    } else {
        files
    }
}

fn check(args: CheckCommand) -> Result<ExitStatus> {
    let files = resolve_default_files(args.files, false); // @@Todo: add stdin support.

    // Fix rules are as follows:
    // - By default, generate all fixes, but don't apply them to the filesystem.
    // - If `--fix` or `--fix-only` is set, apply applicable fixes to the filesystem
    //   (or print them to stdout, if we're reading from stdin).
    // - If `--diff` or `--fix-only` are set, don't print any violations (only
    //   applicable fixes)

    let fix_mode = if args.diff {
        FixMode::Diff
    } else if args.fix {
        FixMode::Apply
    } else {
        FixMode::Generate
    };

    let settings = Settings::new(args.respect_gitignore, fix_mode);
    let _messages = commands::check::check(&files, settings)?;

    // @@TODO: display the actual diagnostics that were collected.
    Ok(ExitStatus::Success)
}

fn version() -> Result<ExitStatus> {
    commands::version::version()?;
    Ok(ExitStatus::Success)
}
