//! Library definition of `bl` crate.

#![feature(panic_payload_as_str)]
mod crash;
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
/// Handler function which will delegate functionality to the appropriate
/// command.
pub fn run(cli::Cli { command }: cli::Cli) -> Result<ExitStatus> {
    // Initial grunt work, panic handler and logger setup...
    panic::set_hook(Box::new(crash_handler));

    let output_stream = CompilerOutputStream::stdout;
    match command {
        cli::Command::Check(args) => check(args),
        cli::Command::Version => version(),
    }
}
fn check(args: CheckCommand) -> Result<ExitStatus> {

    // @@TODO: display the actual diagnostics that were collected.
    Ok(ExitStatus::Success)
}

fn version() -> Result<ExitStatus> {
    commands::version::version()?;
    Ok(ExitStatus::Success)
}
