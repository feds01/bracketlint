//! Bracketlint Compiler crash handler
use std::{io::Write, panic::PanicHookInfo, process::exit, sync::atomic, thread};

const BUG_REPORT_MSG: &str = "This is a bracketlint bug, please file a bug report at";
const BUG_REPORT_URI: &str =
    "https://github.com/feds01/bracketlint/issues?labels=bug&template=bug_report";

/// The Bracketlint crash handler. This function should be used to register a
/// `panic` handle for Bracketlint. It will ensure that only one thread can
/// panic at a time, it will print the panic message, a backtrace, and a link to
/// the bug report page to `stderr`, so that they don't end up in the output of
/// a command, e.g. a diff that is written to a file.
///
/// # Registering the handler
/// ```ignore
/// use std::panic;
/// ...
///
/// panic::set_hook(Box::new(crash_handler));
/// ```
pub fn crash_handler(info: &PanicHookInfo) {
    // keep track to ensure that we only panic once and multiple threads can exit
    // gracefully!
    static PANIC_ONCE: atomic::AtomicBool = atomic::AtomicBool::new(false);

    if !PANIC_ONCE.swap(true, atomic::Ordering::SeqCst) {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();

        let _ = write!(&mut stderr, "Sorry :^(\nInternal Compiler Error");

        if let Some(s) = info.payload().downcast_ref::<&str>() {
            let _ = writeln!(&mut stderr, ": {s}\n");
        } else if let Some(s) = info.payload_as_str() {
            let _ = writeln!(&mut stderr, ": {s}\n");
        } else {
            let _ = writeln!(&mut stderr, "\n");
        }

        // Display the location if we can...
        if let Some(location) = info.location() {
            let _ = writeln!(
                &mut stderr,
                "Occurred at '{}:{}:{}'",
                location.file(),
                location.line(),
                location.column()
            );
        }

        let backtrace = std::backtrace::Backtrace::force_capture();

        // Print backtrace and thread name if available
        if let Some(name) = thread::current().name() {
            let _ = writeln!(&mut stderr, "Backtrace for thread \"{name}\":\n{backtrace}");
        } else {
            let _ = writeln!(&mut stderr, "Backtrace:\n{backtrace}");
        }

        let _ = writeln!(
            &mut stderr,
            "{}\n\n{:^len$}\n",
            BUG_REPORT_MSG,
            BUG_REPORT_URI,
            len = BUG_REPORT_MSG.len()
        );
    }

    // Now call exit after we have printed all the relevant info, with the code
    // of an internal error, see `ExitStatus::Error`.
    exit(2);
}
