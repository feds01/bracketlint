//! Library definition of `bl` crate.

pub mod cli;
mod commands;
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
use cli::LintCommand;
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

fn check(args: LintCommand) -> Result<ExitStatus> {
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

    let settings = Settings::new(args.respect_gitignore, fix_mode, args.dump_ast);
    let builder = WorkspaceBuilder::new()
        .with_settings(settings)
        .with_stdout(CompilerOutputStream::stdout())
        .with_stderr(CompilerOutputStream::stderr());
    let mut workspace = builder.build();

    let messages = commands::check::check(&files, &mut workspace)?;
    consume_diagnostics(&workspace, messages)
}

fn fmt(args: LintCommand) -> Result<ExitStatus> {
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

    let settings = Settings::new(args.respect_gitignore, fix_mode, args.dump_ast);
    let builder = WorkspaceBuilder::new()
        .with_settings(settings)
        .with_stdout(CompilerOutputStream::stdout())
        .with_stderr(CompilerOutputStream::stderr());
    let mut workspace = builder.build();

    let messages = commands::fmt::fmt(&files, &mut workspace)?;
    consume_diagnostics(&workspace, messages)
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

        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
}
