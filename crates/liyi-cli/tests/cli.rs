//! Integration tests for the `liyi` CLI binary.
//!
//! These exercise the compiled binary end-to-end (argument parsing, exit
//! codes, and the top-level command wiring in `main.rs`) rather than the
//! library API — that surface is covered by the golden tests in the `liyi`
//! crate.  Each test runs the binary built for this crate via the
//! `CARGO_BIN_EXE_liyi` path that Cargo injects for integration tests.

use std::path::Path;
use std::process::{Command, Output};

/// Path to the freshly built `liyi` binary for this test run.
fn liyi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_liyi")
}

/// Run `liyi` with the given args in `cwd` and capture its output.
fn run_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(liyi_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn liyi binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------------------------------------------------------------------------
// Top-level argument parsing
// ---------------------------------------------------------------------------

#[test]
fn version_flag_reports_crate_version() {
    let out = Command::new(liyi_bin())
        .arg("--version")
        .output()
        .expect("failed to spawn liyi binary");
    assert!(out.status.success(), "--version should exit 0");
    assert!(
        stdout(&out).contains("0.1.0"),
        "--version output should contain the crate version, got: {:?}",
        stdout(&out)
    );
}

#[test]
fn help_flag_lists_subcommands() {
    let out = Command::new(liyi_bin())
        .arg("--help")
        .output()
        .expect("failed to spawn liyi binary");
    assert!(out.status.success(), "--help should exit 0");
    let text = stdout(&out);
    for sub in ["check", "init", "approve", "migrate"] {
        assert!(text.contains(sub), "--help should mention `{sub}`");
    }
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let out = Command::new(liyi_bin())
        .arg("definitely-not-a-command")
        .output()
        .expect("failed to spawn liyi binary");
    // clap exits with code 2 on argument-parsing errors.
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !stderr(&out).is_empty(),
        "usage errors should be reported on stderr"
    );
}

#[test]
fn no_arguments_prints_usage_and_fails() {
    let out = Command::new(liyi_bin())
        .output()
        .expect("failed to spawn liyi binary");
    // A missing required subcommand is a clap usage error (exit 2).
    assert_eq!(out.status.code(), Some(2));
}

// ---------------------------------------------------------------------------
// `liyi migrate` argument validation
// ---------------------------------------------------------------------------

#[test]
fn migrate_without_paths_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = run_in(tmp.path(), &["migrate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("required"),
        "migrate with no paths should explain a path is required, got: {:?}",
        stderr(&out)
    );
}

// ---------------------------------------------------------------------------
// `liyi init`
// ---------------------------------------------------------------------------

#[test]
fn init_creates_agents_md_when_absent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = run_in(tmp.path(), &["init"]);
    assert!(
        out.status.success(),
        "init should exit 0: {:?}",
        stderr(&out)
    );
    let agents = tmp.path().join("AGENTS.md");
    assert!(agents.is_file(), "init should create AGENTS.md");
    let content = std::fs::read_to_string(&agents).unwrap();
    assert!(
        content.contains("立意"),
        "generated AGENTS.md should contain the liyi template block"
    );
    assert!(
        content.contains("START liyi agent instructions"),
        "generated AGENTS.md should carry the pragma, got: {content}"
    );
}

#[test]
fn init_is_idempotent_for_agents_md() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    let first = run_in(root, &["init"]);
    assert!(first.status.success(), "first init: {:?}", stderr(&first));

    let second = run_in(root, &["init"]);
    assert!(
        !second.status.success(),
        "a second init should refuse rather than duplicate the block"
    );
    assert!(stderr(&second).contains("already exists"));

    let content = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert_eq!(
        content.matches("START liyi agent instructions").count(),
        1,
        "init must not append a duplicate block"
    );
}

#[test]
fn migrate_rewrites_an_older_agents_md_block() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let agents = root.join("AGENTS.md");
    std::fs::write(
        &agents,
        "# AGENTS.md\n\n<!-- START liyi agent instructions rev. 0 -->\n<!-- DON'T EDIT -->\nstale body\n<!-- END liyi agent instructions -->\n",
    )
    .unwrap();

    let out = run_in(root, &["migrate", "AGENTS.md"]);
    assert!(out.status.success(), "migrate: {:?}", stderr(&out));

    let content = std::fs::read_to_string(&agents).unwrap();
    assert!(
        content.contains("rev. 1"),
        "block should be at the current revision, got: {content}"
    );
    assert!(
        content.contains("## The 立意"),
        "block should carry the template, got: {content}"
    );
    assert!(
        !content.contains("stale body"),
        "the stale body should be replaced"
    );
}

#[test]
fn migrate_directory_updates_agents_md_and_sidecars() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("AGENTS.md"),
        "# AGENTS.md\n\n<!-- START liyi agent instructions rev. 0 -->\n<!-- DON'T EDIT -->\nstale body\n<!-- END liyi agent instructions -->\n",
    )
    .unwrap();

    let src = root.join("sample.rs");
    std::fs::write(&src, "pub fn f() {}\n").unwrap();
    run_in(root, &["init", "sample.rs"]);

    let out = run_in(root, &["migrate", "."]);
    assert!(out.status.success(), "migrate: {:?}", stderr(&out));

    let content = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(content.contains("rev. 1"));
    assert!(!content.contains("stale body"));
}

#[test]
fn init_creates_sidecar_for_source_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let src = tmp.path().join("sample.rs");
    std::fs::write(&src, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

    let out = run_in(tmp.path(), &["init", "sample.rs"]);
    assert!(
        out.status.success(),
        "init should exit 0: {:?}",
        stderr(&out)
    );

    let sidecar = tmp.path().join("sample.rs.liyi.jsonc");
    assert!(sidecar.is_file(), "init should create the sidecar file");
    let content = std::fs::read_to_string(&sidecar).unwrap();
    assert!(
        content.contains("\"source\": \"sample.rs\""),
        "sidecar should reference the source file, got: {content}"
    );
    assert!(
        content.contains("add"),
        "discovery should find the `add` function, got: {content}"
    );
}

#[test]
fn init_refuses_to_overwrite_without_force() {
    let tmp = tempfile::TempDir::new().unwrap();
    let src = tmp.path().join("sample.rs");
    std::fs::write(&src, "pub fn f() {}\n").unwrap();

    let first = run_in(tmp.path(), &["init", "sample.rs"]);
    assert!(first.status.success());

    let second = run_in(tmp.path(), &["init", "sample.rs"]);
    assert!(
        !second.status.success(),
        "a second init without --force should fail"
    );
    assert!(stderr(&second).contains("already exists"));

    let forced = run_in(tmp.path(), &["init", "sample.rs", "--force"]);
    assert!(forced.status.success(), "init --force should overwrite");
}

// ---------------------------------------------------------------------------
// `liyi check`
// ---------------------------------------------------------------------------

/// End-to-end happy path: scaffold a sidecar, fill hashes with `--fix`, then a
/// plain `check` should pass with exit code 0.
#[test]
fn check_passes_after_init_and_fix() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let src = root.join("sample.rs");
    std::fs::write(&src, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

    let init = run_in(root, &["init", "sample.rs"]);
    assert!(init.status.success(), "init: {:?}", stderr(&init));

    let fix = run_in(root, &["check", "--fix", "--root", "."]);
    assert!(fix.status.success(), "check --fix: {:?}", stderr(&fix));

    let check = run_in(root, &["check", "--root", "."]);
    assert!(
        check.status.success(),
        "check after fix should pass, stdout: {}, stderr: {}",
        stdout(&check),
        stderr(&check)
    );
}

/// `--prompt` must emit valid JSON on stdout regardless of exit code.
#[test]
fn check_prompt_emits_valid_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let src = root.join("sample.rs");
    std::fs::write(&src, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

    run_in(root, &["init", "sample.rs"]);
    run_in(root, &["check", "--fix", "--root", "."]);

    let out = run_in(root, &["check", "--root", ".", "--prompt"]);
    let text = stdout(&out);
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("--prompt output was not valid JSON ({e}): {text}"));
    assert!(
        parsed.is_object(),
        "--prompt should emit a JSON object, got: {text}"
    );
}

// ---------------------------------------------------------------------------
// `liyi context`
// ---------------------------------------------------------------------------

/// A directory-scope note in a marked `README.md` should be resolved and
/// printed for a source file in the same subtree.
#[test]
fn context_prints_directory_scope_note() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("README.md"),
        "<!-- \x40liyi:note billing -->\nAll amounts carry their currency.\n<!-- \x40liyi:end-note billing -->\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/money.rs"), "fn settle() {}\n").unwrap();

    let out = run_in(root, &["context", "src/money.rs", "--root", "."]);
    assert!(
        out.status.success(),
        "context should exit 0: {:?}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(
        text.contains("billing") && text.contains("All amounts carry their currency."),
        "context should print the applicable note, got: {text}"
    );
}

/// A trailing `:line` component is accepted (MVP resolves file-scoped) and does
/// not change the resolved notes.
#[test]
fn context_accepts_line_suffix() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("README.md"),
        "<!-- \x40liyi:note project -->\nProject-wide invariant.\n<!-- \x40liyi:end-note project -->\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/money.rs"), "fn settle() {}\n").unwrap();

    let out = run_in(root, &["context", "src/money.rs:42", "--root", "."]);
    assert!(
        out.status.success(),
        "context with :line should exit 0: {:?}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("Project-wide invariant."),
        "context should resolve the note regardless of the :line suffix, got: {}",
        stdout(&out)
    );
}

/// With no marked notes in scope, context reports that none apply and still
/// exits 0 (resolution is best-effort, never a hard failure).
#[test]
fn context_reports_when_no_notes_apply() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("README.md"), "# Manual\n\nNo marker here.\n").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/money.rs"), "fn f() {}\n").unwrap();

    let out = run_in(root, &["context", "src/money.rs", "--root", "."]);
    assert!(
        out.status.success(),
        "context with no notes should still exit 0: {:?}",
        stderr(&out)
    );
    assert!(
        stdout(&out).to_lowercase().contains("no notes"),
        "context should report that no notes apply, got: {}",
        stdout(&out)
    );
}

/// A target outside the repo root is a usage error (exit 2).
#[test]
fn context_rejects_target_outside_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(root.join("outside.rs"), "fn f() {}\n").unwrap();

    // Root is repo/, but the target lives above it.
    let repo_arg = repo.to_str().unwrap();
    let out = run_in(root, &["context", "outside.rs", "--root", repo_arg]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a target outside the repo root should be a usage error, stderr: {}",
        stderr(&out)
    );
}
