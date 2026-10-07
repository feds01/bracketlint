//! UI Test runner, this file will read the `cases` directory for
//! `.html` files.
//!
//! In the event of a `should_pass` case, the resultant run should not return
//! any errors. However, it's possible for the case to generate warnings
//! (which is not currently tested).
//!
//! In the event of a `should_fail` case, the test will emit "reports" that
//! signal what the error is, the case handler will render the reports
//! into strings, strip any kind of ANSI codes, and compare with the
//! `case.stderr` to ensure that the test produces the expected errors.
//!
//! Additionally, it's also possible for the case file to specify at the
//! top of the file what testing parameters should be provided, for example:
//! ```ignore
//! // stage=check, run=fail
//!
//! {% block %} Hello World! {% endblock %}
//! ```
//!
//! In this example, the case specifies that the stage should only go up to the
//! "check" (parsing) stage and then stop processing, and that the test case
//! should fail. The keys are parsed in `testing-internal/src/metadata.rs`.
//!
//! The configuration line isn't part of the template, so the case runs on a
//! copy without it, or the blank line that follows it. Line numbers in the
//! snapshots count from the first line of the template.
//!
//! A case can also set the template dialect to parse it in, e.g.
//! `dialect=liquid`. Otherwise it is parsed as Django, like any `.html` file.
#![cfg(test)]

use std::{
    fs, io,
    path::{MAIN_SEPARATOR, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use bl_lints::settings::FixMode;
use bl_reporting::{Report, Reporter};
use bl_testing_internal::{
    TestingInput,
    metadata::{HandleWarnings, StageKind, TestResult, strip_config_line},
};
use bl_testing_macros::generate_tests;
use bl_utils::{
    logging::{MessagingFormat, ToolLogger},
    path::adjust_canonicalisation,
    stream::CompilerOutputStream,
};
use bl_workspace::{Workspace, WorkspaceBuilder, settings::Settings};
use bracketlint::commands;
use tempfile::TempDir;

use crate::{ANSI_REGEX, REGENERATE_OUTPUT};

/// The directory that the case runs in, which `$DIR` stands for in the
/// snapshots. Each case runs in a process of its own, so there is only one.
static CASE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The copy of a case that the case runs on, without its configuration line.
/// It lives in a temporary directory, which is removed when the copy is
/// dropped.
struct CaseCopy {
    /// The path of the copied case.
    path: PathBuf,

    /// The directory that the copy lives in.
    _dir: TempDir,
}

impl CaseCopy {
    /// Copy the case at `test.path` into a temporary directory, without the
    /// configuration line at the top of it.
    fn new(test: &TestingInput) -> io::Result<Self> {
        let contents = fs::read_to_string(&test.path)?;
        let dir = tempfile::Builder::new().prefix("bracketlint-ui-").tempdir()?;

        // The workspace reports canonical paths, so `$DIR` has to be one too.
        CASE_DIR.set(adjust_canonicalisation(dir.path())).expect("one case per process");
        let path = CASE_DIR.get().unwrap().join(test.path.file_name().unwrap());
        fs::write(&path, strip_config_line(&contents))?;

        Ok(Self { path, _dir: dir })
    }
}

/// This function will strip the provided content string of all ANSI escape
/// codes, and replace all references to the directory that the case ran in
/// with `$DIR`.
fn strip_contents(contents: &str) -> String {
    // Remove any ANSI escape codes generated from the reporting...
    let stripped = ANSI_REGEX.replace_all(contents, "");

    // Replace the directory by `$DIR`, followed by a `/` on every platform.
    let dir = format!("{}{MAIN_SEPARATOR}", CASE_DIR.get().unwrap().display());
    stripped.replace(&dir, "$DIR/").replace("\r\n", "\n")
}

/// Given the testing input, and a pre-filtered [Vec<Report>] based on
/// the testing input parameters, render the reports and compare them
/// to the saved corresponding `.stderr` file.
fn compare_emitted_diagnostics(
    input: &TestingInput,
    workspace: &Workspace,
    diagnostics: Vec<Report>,
) -> std::io::Result<()> {
    // First, convert the diagnostics into a string.
    let contents = format!("{}", Reporter::new(workspace, diagnostics));

    compare_output(input, OutputKind::Stderr, contents.as_str())
}

/// This enum is used to specify the kind of output that we are comparing
/// against. This is used to determine which file we are comparing against
/// and also to determine which file we are writing to.
enum OutputKind {
    /// We're comparing the output of the parser to the `.stderr` file
    Stderr,

    /// We're comparing the output of the parser to the `.stdout` file
    Stdout,
}

impl OutputKind {
    /// Get the appropriate extension for the [OutputKind].
    pub fn extension(&self) -> &'static str {
        match self {
            OutputKind::Stderr => "stderr",
            OutputKind::Stdout => "stdout",
        }
    }
}

/// This function will compare the provided contents to the corresponding saved
/// output file, which is either a `.stderr` or `.stdout` file (which is
/// specified by the [OutputKind]). The provided contents are stripped of ANSI
/// escape codes and all directory paths are replaced with `$DIR`.
///
/// If the file does not exist, then it will be created and the contents will be
/// written to it.
///
/// If [`REGENERATE_OUTPUT`] is set to `true`, then the file will be overwritten
/// with the new stripped contents.
fn compare_output(test: &TestingInput, kind: OutputKind, contents: &str) -> std::io::Result<()> {
    let actual_contents = strip_contents(contents);

    // We want to load the `.{stderr|stdout}` file and verify that the contents of
    // the file match to the created report. If the `.stderr` file does not
    // exist then we create it and write the generated report to that file
    let test_dir = test.path.parent().unwrap();
    let content_path = test_dir.join(format!("{}.{}", test.filename, kind.extension()));

    // If we specify to re-generate the output, then we will always write the
    // content of the report into the specified file
    if *REGENERATE_OUTPUT || !content_path.exists() {
        // Avoid writing nothing to the `.stderr` case file
        if !actual_contents.is_empty() {
            fs::write(&content_path, &actual_contents)?;
        }
    }

    // We want to delete the file if the contents are empty, and we are
    // re-generating
    if *REGENERATE_OUTPUT && content_path.exists() && actual_contents.is_empty() {
        fs::remove_file(&content_path)?;
    }

    // Read the contents of the file, if we couldn't find the file then we assume
    // that the contents of `.stderr` are empty. We can assume this because the
    // only reason why the file wouldn't be written to is if it didn't exist
    // prior to this and that the contents of the diagnostics are empty, so
    // returning an empty string makes sense here.
    let expected_contents = fs::read_to_string(content_path.clone())
        .unwrap_or_else(|err| match err.kind() {
            io::ErrorKind::NotFound => "".to_string(),
            err => panic!("couldn't open file `{content_path:?}`: {:?}", err),
        })
        .replace("\r\n", "\n");

    pretty_assertions::assert_str_eq!(
        expected_contents,
        actual_contents,
        "\ncase `.{}` does not match for: {:#?}\n",
        kind.extension(),
        test
    );

    Ok(())
}

fn compare_stream(
    test: &TestingInput,
    kind: OutputKind,
    output_stream: &Arc<Mutex<Vec<u8>>>,
) -> io::Result<()> {
    let stream = output_stream.lock().unwrap();
    let contents = std::str::from_utf8(&stream).unwrap();

    compare_output(test, kind, contents)
}

/// This function is used to handle the case of verifying that a parser test was
/// expected to fail. This function verifies that it does fail and that the
/// generated [Report] (which is rendered) matches the recorded `case.stderr`
/// entry within the case.
///
/// If the case specifies that `warnings=ignore`, then warnings will not be
/// considered within the resultant `.stderr` file.
fn handle_failure_case(
    test: TestingInput,
    workspace: &Workspace,
    diagnostics: Vec<Report>,
    output_stream: &Arc<Mutex<Vec<u8>>>,
) -> std::io::Result<()> {
    // verify that the case failed, as in reports where generated
    assert!(
        diagnostics.iter().any(|report| report.is_error()),
        "\ntest case did not fail: {test:#?}{}",
        ""
    );

    // If the test specifies that no warnings should be generated, then check
    // that this is the case
    if test.metadata.warnings == HandleWarnings::Disallow {
        assert!(
            diagnostics.iter().all(|report| report.is_error()),
            "\ntest case generated warnings where they were disallowed: {test:#?}{}",
            ""
        );
    }

    // Filter out `warnings` if the function specifies them to be
    // ignored.
    let diagnostics = diagnostics
        .into_iter()
        .filter(|report| {
            if test.metadata.warnings == HandleWarnings::Ignore { report.is_error() } else { true }
        })
        .collect();

    compare_emitted_diagnostics(&test, workspace, diagnostics)?;
    compare_stream(&test, OutputKind::Stdout, output_stream)
}

/// Function that handles a test case which is expected to be successful, in
/// this situation, the function will verify that the test case did not emit any
/// errors or warnings (although setting `warnings=ignore` will ignore
/// warnings).
fn handle_pass_case(
    test: TestingInput,
    workspace: &Workspace,
    diagnostics: Vec<Report>,
    output_stream: &Arc<Mutex<Vec<u8>>>,
) -> std::io::Result<()> {
    let did_pass = match test.metadata.warnings {
        HandleWarnings::Ignore | HandleWarnings::Compare => {
            diagnostics.iter().all(|report| report.is_warning())
        }
        // Expect no diagnostics to be emitted whatsoever
        HandleWarnings::Disallow => diagnostics.is_empty(),
    };

    if !did_pass {
        panic!(
            "\ntest case did not pass:\nconfiguration: {:?}\n\n{}",
            test.metadata,
            Reporter::new(workspace, diagnostics)
        );
    }

    // If we need to compare the output of the warnings, to the previous result...
    if test.metadata.warnings == HandleWarnings::Compare {
        compare_emitted_diagnostics(&test, workspace, diagnostics)?;
    }

    compare_stream(&test, OutputKind::Stdout, output_stream)
}

static LOGGER: ToolLogger = ToolLogger::new();

/// Generic test handler in the event whether a case should pass or fail.
fn handle_test(test: TestingInput) {
    let raw_output_stream = Arc::new(Mutex::new(Vec::new()));

    // Build the output streams for the workspace.
    let output_stream = || {
        let output_stream = raw_output_stream.clone();
        CompilerOutputStream::Owned(output_stream.clone())
    };
    let error_stream = CompilerOutputStream::stderr;

    // Setup the logger for the test case.
    LOGGER.error_stream.set(error_stream()).unwrap();
    LOGGER.output_stream.set(output_stream()).unwrap();
    LOGGER.set_messaging_format(MessagingFormat::Normal);
    log::set_logger(&LOGGER).unwrap_or_else(|_| panic!("couldn't initiate logger"));
    log::set_max_level(log::LevelFilter::Info);

    // Create a workspace for the test case, which runs on a copy of the case
    // without its configuration line.
    let case = CaseCopy::new(&test).unwrap();
    let files = vec![case.path.clone()];
    let settings = Settings::new(true, FixMode::Generate, false, test.metadata.dialect);
    let builder = WorkspaceBuilder::new()
        .with_settings(settings)
        .with_stdout(output_stream())
        .with_stderr(error_stream());
    let mut workspace = builder.build();

    // Run the appropriate stage for the test case.
    let messages = match test.metadata.stage {
        StageKind::Check => {
            // @@Hack: make this configurable within the test case itself.
            workspace.settings.parser_settings.dump_ast = true;
            commands::check::check(&files, &mut workspace).unwrap()
        }
        StageKind::Fmt => commands::fmt::fmt(&files, &mut workspace).unwrap(),
    };

    // Based on the specified metadata within the test case itself, we know
    // whether the test should fail or not
    if test.metadata.completion == TestResult::Fail {
        handle_failure_case(test, &workspace, messages, &raw_output_stream).unwrap();
    } else {
        handle_pass_case(test, &workspace, messages, &raw_output_stream).unwrap();
    }
}

// Generate all the tests
generate_tests!("./cases/", r"^*\.html$", "ui", handle_test);
