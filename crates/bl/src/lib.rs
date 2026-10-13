//! Library definition of the `bracketlint` crate.

pub mod cli;
pub mod commands;
mod crash;
pub(crate) mod version;

use std::{
    io::Write,
    panic,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Ok, Result};
use bl_lints::settings::FixMode;
use bl_reporting::{Reporter, Reports, pluralise};
use bl_utils::{logging::ToolLogger, stream::CompilerOutputStream, stream_writeln};
use bl_workspace::{Workspace, WorkspaceBuilder, settings::Settings};
use cli::{CheckCommand, FormatCommand};
use crash::crash_handler;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    /// Linting was successful and there were no linting errors or warnings.
    Success,
    /// Linting was successful and there were linting warnings, but no errors.
    /// A warning doesn't fail the run, unless `--deny-warnings` is passed.
    Warnings,
    /// Linting was successful but there were linting errors.
    Failure,
    /// Linting failed.
    Error,
}

impl ExitStatus {
    /// The status when warnings fail the run, as `--deny-warnings` asks.
    fn deny_warnings(self) -> Self {
        match self {
            ExitStatus::Warnings => ExitStatus::Failure,
            status => status,
        }
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success | ExitStatus::Warnings => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
        }
    }
}

pub static LOGGER: ToolLogger = ToolLogger::new();

/// Handler function which will delegate functionality to the appropriate
/// command.
pub fn run(cli::Cli { command, verbose }: cli::Cli) -> Result<ExitStatus> {
    // Initial grunt work, panic handler and logger setup...
    panic::set_hook(Box::new(crash_handler));

    log::set_logger(&LOGGER).unwrap_or_else(|_| panic!("couldn't initiate logger"));

    // Logs go to `stderr`, so that `stdout` only has what the command outputs,
    // e.g. `bracketlint fmt --diff > changes.diff`. The info logs are the tree
    // that `--dump-ast` asks for, and the count of errors and warnings, so
    // only debug logs, such as timings, wait for `--verbose`.
    LOGGER.error_stream.set(CompilerOutputStream::stderr()).unwrap();
    log::set_max_level(if verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info });

    // We also need to create a global-config

    match command {
        cli::Command::Check(args) => check(args),
        cli::Command::Fmt(args) => fmt(args),
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

pub fn check(args: CheckCommand) -> Result<ExitStatus> {
    let CheckCommand { files, dump_ast, shared } = args;
    let files = resolve_default_files(files, false); // @@Todo: add stdin support.

    // There are no lint rules yet, so there are no fixes to apply.
    let settings =
        Settings::new(shared.respect_gitignore(), FixMode::default(), dump_ast, shared.dialect);
    let mut workspace = build_workspace(settings);

    let messages = commands::check::check(&files, &mut workspace)?;
    let status = consume_diagnostics(&workspace, messages)?;
    Ok(if shared.deny_warnings { status.deny_warnings() } else { status })
}

fn fmt(args: FormatCommand) -> Result<ExitStatus> {
    let FormatCommand { files, fix, diff, shared } = args;
    let files = resolve_default_files(files, false); // @@Todo: add stdin support.

    let fix_mode = if diff {
        FixMode::Diff
    } else if fix {
        FixMode::Apply
    } else {
        FixMode::Generate
    };

    let settings = Settings::new(shared.respect_gitignore(), fix_mode, false, shared.dialect);
    let mut workspace = build_workspace(settings);

    let messages = commands::fmt::fmt(&files, &mut workspace)?;
    let status = consume_diagnostics(&workspace, messages)?;
    Ok(if shared.deny_warnings { status.deny_warnings() } else { status })
}

/// A workspace with the given `settings`, which writes to `stdout` and
/// `stderr`.
fn build_workspace(settings: Settings) -> Workspace {
    WorkspaceBuilder::new()
        .with_settings(settings)
        .with_stdout(CompilerOutputStream::stdout())
        .with_stderr(CompilerOutputStream::stderr())
        .build()
}

fn version() -> Result<ExitStatus> {
    commands::version::version()?;
    Ok(ExitStatus::Success)
}

/// Emit diagnostics to the error stream with the applied settings.
fn consume_diagnostics(workspace: &Workspace, diagnostics: Reports) -> Result<ExitStatus> {
    let mut err_count = 0;
    let mut warn_count = 0;
    let mut stderr = workspace.error_stream();

    for diagnostic in diagnostics.iter() {
        if diagnostic.is_error() {
            err_count += 1;
        }

        if diagnostic.is_warning() {
            warn_count += 1;
        }
    }

    if !diagnostics.is_empty() {
        stream_writeln!(stderr, "{}", Reporter::new(workspace, diagnostics));
    }

    // ##Hack: to prevent the compiler from printing this message when the pipeline
    // when it was instructed to terminate before all of the stages. For example, if
    // the compiler is just checking the source, then it will terminate early.
    if err_count != 0 || warn_count != 0 {
        log::info!(
            "bracketlint terminated with {err_count} error{}, and {warn_count} warning{}",
            pluralise!(err_count),
            pluralise!(warn_count)
        );
    }

    // A warning points out something that is likely wrong, such as another
    // dialect's syntax, but doesn't stop a template from being checked or
    // formatted, so only errors fail the run, unless `--deny-warnings` is
    // passed.
    if err_count != 0 {
        Ok(ExitStatus::Failure)
    } else if warn_count != 0 {
        Ok(ExitStatus::Warnings)
    } else {
        Ok(ExitStatus::Success)
    }
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_deny_warnings() {
        assert_eq!(ExitStatus::Warnings.deny_warnings(), ExitStatus::Failure);
        assert_eq!(ExitStatus::Success.deny_warnings(), ExitStatus::Success);
        assert_eq!(ExitStatus::Failure.deny_warnings(), ExitStatus::Failure);
    }
}
