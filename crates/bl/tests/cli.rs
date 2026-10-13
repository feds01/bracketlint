//! Tests of the `bracketlint` binary: what it prints to stdout and to stderr,
//! the files that it writes, and the code that it exits with.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// A template that formatting changes, as its `{% endif %}` is indented.
const UNFORMATTED: &str = "{% if user %}\n<p>{{ user.name }}</p>\n      {% endif %}\n";

/// A template that fails to parse, as its `{% if %}` isn't closed.
const BROKEN: &str = "{% if user %}\n<p>{{ user.name }}</p>\n";

/// A directory for the test `name` alone, with the given `files` in it.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    for (path, contents) in files {
        let path = dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    dir
}

/// Run `bracketlint` in `dir` with `args`.
fn bracketlint(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bracketlint")).args(args).current_dir(dir).output().unwrap()
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

/// The path of the template that the tests format, as the binary names it.
fn template_path() -> String {
    Path::new("templates").join("page.html").display().to_string()
}

#[test]
fn version_names_bracketlint() {
    let dir = project("version_names_bracketlint", &[]);
    let expected = format!("bracketlint {}\n", env!("CARGO_PKG_VERSION"));

    for args in [&["--version"][..], &["version"]] {
        let output = bracketlint(&dir, args);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(stdout(&output), expected, "`bracketlint {}`", args[0]);
    }
}

#[test]
fn fmt_writes_templates_that_formatting_changes() {
    let dir = project("fmt_writes", &[("templates/page.html", UNFORMATTED)]);
    let path = dir.join("templates/page.html");

    let output = bracketlint(&dir, &["fmt"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "1 file reformatted\n");
    assert_ne!(fs::read_to_string(&path).unwrap(), UNFORMATTED);

    // A template that is already formatted isn't written again, so `fmt`
    // doesn't fail on one that it couldn't write.
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    let output = bracketlint(&dir, &["fmt", "templates"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "1 file left unchanged\n");
}

#[test]
fn fmt_check_names_templates_that_formatting_would_change() {
    let dir = project("fmt_check", &[("templates/page.html", UNFORMATTED)]);

    let output = bracketlint(&dir, &["fmt", "--check", "templates"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), format!("Would reformat: {}\n", template_path()));
    assert_eq!(stderr(&output), "1 file would be reformatted\n");
    assert_eq!(fs::read_to_string(dir.join("templates/page.html")).unwrap(), UNFORMATTED);

    // Once the template is formatted, there is nothing to name.
    assert_eq!(bracketlint(&dir, &["fmt", "templates"]).status.code(), Some(0));
    let output = bracketlint(&dir, &["fmt", "--check", "templates"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "1 file already formatted\n");
}

#[test]
fn fmt_diff_prints_a_plain_diff_to_stdout() {
    let dir = project("fmt_diff", &[("templates/page.html", UNFORMATTED)]);

    let output = bracketlint(&dir, &["fmt", "--diff", "templates"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stderr(&output), "1 file would be reformatted\n");
    assert_eq!(fs::read_to_string(dir.join("templates/page.html")).unwrap(), UNFORMATTED);

    // The diff names the file, and has no colours, as stdout isn't a terminal.
    let diff = stdout(&output);
    let path = template_path();
    assert!(diff.starts_with(&format!("--- {path}\n+++ {path}\n@@ ")), "{diff}");
    assert!(diff.contains("\n-      {% endif %}\n"), "{diff}");
    assert!(!diff.contains('\x1b'), "{diff:?}");
}

#[test]
fn logs_go_to_stderr_and_debug_logs_wait_for_verbose() {
    let dir = project("logs", &[("templates/page.html", UNFORMATTED)]);

    let output = bracketlint(&dir, &["fmt", "--diff", "templates"]);
    assert!(!stderr(&output).contains("debug"), "{}", stderr(&output));

    let output = bracketlint(&dir, &["fmt", "--diff", "-v", "templates"]);
    assert!(stderr(&output).contains("debug"), "{}", stderr(&output));
    assert!(!stdout(&output).contains("debug"), "{}", stdout(&output));
}

#[test]
fn a_missing_path_fails_the_run() {
    let dir = project("missing_path", &[]);

    for command in ["check", "fmt"] {
        let output = bracketlint(&dir, &[command, "missing.html"]);
        assert_eq!(output.status.code(), Some(1), "`{command}`: {}", stderr(&output));
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).contains("couldn't read `missing.html`"), "{}", stderr(&output));
    }
}

#[test]
fn no_templates_is_a_warning() {
    let dir = project("no_templates", &[("notes.txt", "Not a template.\n")]);

    for command in ["check", "fmt"] {
        let output = bracketlint(&dir, &[command]);
        assert_eq!(output.status.code(), Some(0), "`{command}`: {}", stderr(&output));
        assert!(stderr(&output).contains("no template files found"), "{}", stderr(&output));
    }
}

#[test]
fn ignored_files_are_skipped_unless_asked_for() {
    let dir =
        project("ignored_files", &[(".ignore", "generated/\n"), ("generated/page.html", BROKEN)]);

    let output = bracketlint(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let output = bracketlint(&dir, &["check", "--no-respect-gitignore"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn reports_in_a_pipe_are_not_coloured() {
    let dir = project("not_coloured", &[("templates/page.html", BROKEN)]);

    let output = bracketlint(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("error"), "{}", stderr(&output));
    assert!(!stderr(&output).contains('\u{1b}'), "{}", stderr(&output));
}

#[test]
fn a_missing_file_of_arguments_fails_the_run() {
    let dir = project("missing_argfile", &[]);

    let output = bracketlint(&dir, &["check", "@missing-arguments"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("couldn't read the arguments"), "{}", stderr(&output));
}

#[test]
fn invalid_project_settings_fail_the_run() {
    let dir = project("invalid_settings", &[("bracketlint.toml", "markup = \"markdown\"\n")]);

    for command in ["check", "fmt"] {
        let output = bracketlint(&dir, &[command]);
        assert_eq!(output.status.code(), Some(2), "`{command}`: {}", stderr(&output));
        assert!(stderr(&output).contains("bracketlint.toml` isn't valid"), "{}", stderr(&output));
        assert!(stderr(&output).contains("unknown variant `markdown`"), "{}", stderr(&output));
    }
}
