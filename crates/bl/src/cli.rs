//! Definitions of the command line interface for the `bl` binary.

use std::path::PathBuf;

use clap::{command, Parser};

#[derive(Debug, Parser)]
#[command(
    author,
    name = "ruff",
    about = "Bracketlint: An fast Jinja template linter and code formatter.",
    after_help = "For help with a specific command, see: `bl help <command>`."
)]
#[command(version)]

pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// The check command checks the given files or directories for linting
    /// errors.
    Check(CheckCommand),

    /// Command to print the version of the `bl` binary.
    Version,
}

#[derive(Clone, Debug, clap::Parser)]
pub struct CheckCommand {
    /// List of files or directories to check.
    #[clap(help = "List of files or directories to check [default: .]")]
    pub files: Vec<PathBuf>,

    /// Apply fixes to resolve lint violations.
    /// Use `--no-fix` to disable or `--unsafe-fixes` to include unsafe fixes.
    #[arg(long, overrides_with("no_fix"))]
    pub fix: bool,

    #[clap(long, overrides_with("fix"), hide = true)]
    no_fix: bool,

    /// Enable preview mode; checks will include unstable rules and fixes.
    /// Use `--no-preview` to disable.
    #[arg(long, overrides_with("no_preview"))]
    pub preview: bool,
    #[clap(long, overrides_with("preview"), hide = true)]
    no_preview: bool,

    /// Respect file exclusions via `.gitignore` and other standard ignore
    /// files. Use `--no-respect-gitignore` to disable.
    #[arg(long, overrides_with("no_respect_gitignore"), help_heading = "File selection")]
    pub respect_gitignore: bool,

    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,

    /// Avoid writing any fixed files back; instead, output a diff for each
    /// changed file to stdout, and exit 0 if there are no diffs.
    /// Implies `--fix-only`.
    #[arg(long, conflicts_with = "show_fixes")]
    pub diff: bool,

    /// Show an enumeration of all fixed lint violations.
    /// Use `--no-show-fixes` to disable.
    #[arg(long, overrides_with("no_show_fixes"))]
    show_fixes: bool,
    #[clap(long, overrides_with("show_fixes"), hide = true)]
    no_show_fixes: bool,
}
