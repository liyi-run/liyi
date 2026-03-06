//! Golden-file integration tests for `liyi check`.
//!
//! Each fixture under `tests/fixtures/` is a self-contained mini directory
//! with source files and `.liyi.jsonc` sidecars.  The test runner calls
//! `run_check` directly (library API) and asserts on diagnostic kinds and
//! exit codes — not exact output strings.
//!
//! Tests that use `fix = true` copy the fixture to a temporary directory
//! first so that the canonical fixtures are never modified.

use std::fs;
use std::path::{Path, PathBuf};

use liyi::check::run_check;
use liyi::diagnostics::{CheckFlags, DiagnosticKind, LiyiExitCode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn default_flags() -> CheckFlags {
    CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
    }
}

/// Recursively copy a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

/// Copy a fixture to a temporary directory and return the temp dir handle
/// (kept alive for the duration of the test) and the root path inside it.
fn fixture_in_tmp(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture_path(name);
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = tmp.path().join(name);
    copy_dir_all(&src, &dest);
    (tmp, dest)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn basic_pass() {
    // Copy to tmp so --fix doesn't mutate the fixture.
    let (_tmp, root) = fixture_in_tmp("basic_pass");
    let flags = default_flags();

    // First run: fix to fill in source_hash / source_anchor.
    let _ = run_check(&root, &[], true, &flags);

    // Second run: everything should be clean.
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);

    let failures: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            !matches!(
                d.kind,
                DiagnosticKind::Current
                    | DiagnosticKind::Trivial
                    | DiagnosticKind::Ignored
                    | DiagnosticKind::Unreviewed // lenient — flag is off
            )
        })
        .collect();

    assert!(failures.is_empty(), "unexpected diagnostics: {failures:#?}");
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

#[test]
fn stale_hash() {
    let root = fixture_path("stale_hash");
    let flags = default_flags();
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);

    let has_stale = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Stale));
    assert!(
        has_stale,
        "expected Stale diagnostic, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

#[test]
fn unreviewed_lenient() {
    // Copy to tmp so --fix doesn't mutate the fixture.
    let (_tmp, root) = fixture_in_tmp("unreviewed");

    let flags = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
    };

    // Fix hashes first
    let _ = run_check(&root, &[], true, &flags);

    // Check: Unreviewed should appear but exit code should be Clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);
    let has_unreviewed = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Unreviewed));
    assert!(
        has_unreviewed,
        "expected Unreviewed diagnostic, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

#[test]
fn unreviewed_strict() {
    // Copy to tmp so --fix doesn't mutate the fixture.
    let (_tmp, root) = fixture_in_tmp("unreviewed");

    let flags_fix = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
    };
    // Fix hashes first
    let _ = run_check(&root, &[], true, &flags_fix);

    let flags_strict = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: true,
        fail_on_req_changed: true,
    };
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags_strict);
    let has_unreviewed = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Unreviewed));
    assert!(has_unreviewed, "expected Unreviewed diagnostic");
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

#[test]
fn orphaned_source() {
    let root = fixture_path("orphaned_source");
    let flags = default_flags();
    let (diagnostics, _exit_code) = run_check(&root, &[], false, &flags);

    let has_orphaned = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::OrphanedSource));
    assert!(
        has_orphaned,
        "expected OrphanedSource diagnostic, got: {diagnostics:#?}"
    );
}

#[test]
fn malformed_jsonc() {
    let root = fixture_path("malformed_jsonc");
    let flags = default_flags();
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);

    let has_parse_error = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::ParseError { .. }));
    assert!(
        has_parse_error,
        "expected ParseError diagnostic, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::InternalError);
}

#[test]
fn trivial_ignore() {
    let (_tmp, root) = fixture_in_tmp("trivial_ignore");
    let flags = default_flags();
    let (diagnostics, _) = run_check(&root, &[], true, &flags);

    let has_trivial = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Trivial));
    let has_ignored = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Ignored));
    assert!(
        has_trivial,
        "expected Trivial diagnostic, got: {diagnostics:#?}"
    );
    assert!(
        has_ignored,
        "expected Ignored diagnostic, got: {diagnostics:#?}"
    );
}

#[test]
fn span_past_eof() {
    let root = fixture_path("span_past_eof");
    let flags = default_flags();
    let (diagnostics, _) = run_check(&root, &[], false, &flags);

    let has_span_err = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::SpanPastEof { .. }));
    assert!(
        has_span_err,
        "expected SpanPastEof diagnostic, got: {diagnostics:#?}"
    );
}

#[test]
fn fullwidth_markers() {
    let (_tmp, root) = fixture_in_tmp("fullwidth_markers");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    let (diagnostics, _) = run_check(&root, &[], true, &flags);

    let has_trivial = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Trivial));
    assert!(
        has_trivial,
        "expected full-width @liyi:trivial marker to be recognized, got: {diagnostics:#?}"
    );
}

#[test]
fn multilingual_aliases() {
    let (_tmp, root) = fixture_in_tmp("multilingual_aliases");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    let (diagnostics, _) = run_check(&root, &[], true, &flags);

    let has_ignored = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Ignored));
    assert!(
        has_ignored,
        "expected Chinese alias @立意:忽略 to be recognized as Ignored, got: {diagnostics:#?}"
    );
}

#[test]
fn shifted_span() {
    let (_tmp, root) = fixture_in_tmp("shifted_span");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    let (diagnostics, _) = run_check(&root, &[], false, &flags);

    let has_shifted = diagnostics.iter().any(|d| {
        matches!(
            d.kind,
            DiagnosticKind::Shifted {
                from: [1, 3],
                to: [4, 6],
            }
        )
    });
    assert!(
        has_shifted,
        "expected Shifted diagnostic from [1,3] to [4,6], got: {diagnostics:#?}"
    );
}

#[test]
fn shifted_span_fix() {
    let (_tmp, root) = fixture_in_tmp("shifted_span");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    // Fix should auto-correct the span
    let _ = run_check(&root, &[], true, &flags);
    // Re-check should be clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);

    let has_current = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Current));
    assert!(
        has_current,
        "expected Current after fix, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

#[test]
fn tree_path_recovery() {
    let (_tmp, root) = fixture_in_tmp("tree_path_recovery");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    let (diagnostics, _) = run_check(&root, &[], false, &flags);

    // tree_path should recover the span from [1,3] to [5,7]
    let has_shifted = diagnostics.iter().any(|d| {
        matches!(
            d.kind,
            DiagnosticKind::Shifted {
                from: [1, 3],
                to: [5, 7],
            }
        )
    });
    assert!(
        has_shifted,
        "expected tree_path Shifted diagnostic from [1,3] to [5,7], got: {diagnostics:#?}"
    );
}

#[test]
fn tree_path_recovery_fix() {
    let (_tmp, root) = fixture_in_tmp("tree_path_recovery");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
    };
    // Fix should auto-correct the span via tree_path
    let _ = run_check(&root, &[], true, &flags);
    // Re-check should be clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, &flags);

    let has_current = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Current));
    assert!(
        has_current,
        "expected Current after tree_path fix, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}
