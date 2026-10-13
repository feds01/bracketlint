//! Definitions of the command line interface for the `bracketlint` binary.

use std::path::PathBuf;

use bl_ast::Dialect;
use clap::{
    Parser,
    builder::{PossibleValuesParser, TypedValueParser},
};

#[derive(Debug, Parser)]
#[command(
    author,
    name = "bracketlint",
    about,
    after_help = "For help with a specific command, see: `bracketlint help <command>`."
)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    /// Also print debug logs, such as how long each step of the run takes.
    #[arg(short, long, global = true, help_heading = "Log levels")]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Report problems in the templates in the given files or directories.
    Check(CheckCommand),

    /// Format the templates in the given files or directories.
    Fmt(FormatCommand),

    /// Print the version of bracketlint.
    Version,
}

#[derive(Clone, Debug, clap::Args)]
pub struct CheckCommand {
    /// The files or directories to check [default: .]
    pub files: Vec<PathBuf>,

    /// Dump the generated AST for the given files.
    #[arg(long)]
    pub dump_ast: bool,

    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Clone, Debug, clap::Args)]
pub struct FormatCommand {
    /// The files or directories to format [default: .]
    pub files: Vec<PathBuf>,

    /// Write the formatted templates back to their files, rather than print
    /// them.
    #[arg(long)]
    pub fix: bool,

    /// Print a diff of how formatting changes each template, rather than the
    /// formatted template.
    #[arg(long, conflicts_with = "fix")]
    pub diff: bool,

    #[command(flatten)]
    pub shared: SharedArgs,
}

/// The arguments that `check` and `fmt` share.
#[derive(Clone, Debug, clap::Args)]
pub struct SharedArgs {
    /// Respect file exclusions via `.gitignore` and other standard ignore
    /// files. Use `--no-respect-gitignore` to disable.
    #[arg(long, overrides_with("no_respect_gitignore"), help_heading = "File selection")]
    pub respect_gitignore: bool,

    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,

    /// Exit with 1 when there are warnings, as when there are errors. By
    /// default, only errors fail the run.
    #[arg(long)]
    pub deny_warnings: bool,

    /// The template dialect to parse files in. By default it is picked from
    /// each file's extension (`.jinja`, `.liquid`, `.njk` or `.twig`), and is
    /// `django` for any other file.
    #[arg(
        long,
        value_parser = PossibleValuesParser::new(Dialect::ALL.iter().map(|dialect| dialect.name()))
            .map(|name| name.parse::<Dialect>().unwrap())
    )]
    pub dialect: Option<Dialect>,
}
