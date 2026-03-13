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
        fail_on_untracked: true,
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
    let _ = run_check(&root, &[], true, false, &flags);

    // Second run: everything should be clean.
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

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
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

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
        fail_on_untracked: true,
    };

    // Fix hashes first
    let _ = run_check(&root, &[], true, false, &flags);

    // Check: Unreviewed should appear but exit code should be Clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);
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
        fail_on_untracked: true,
    };
    // Fix hashes first
    let _ = run_check(&root, &[], true, false, &flags_fix);

    let flags_strict = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: true,
        fail_on_req_changed: true,
        fail_on_untracked: true,
    };
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags_strict);
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
    let (diagnostics, _exit_code) = run_check(&root, &[], false, false, &flags);

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
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

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
    let (diagnostics, _) = run_check(&root, &[], true, false, &flags);

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
    let (diagnostics, _) = run_check(&root, &[], false, false, &flags);

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
        fail_on_untracked: false,
    };
    let (diagnostics, _) = run_check(&root, &[], true, false, &flags);

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
        fail_on_untracked: false,
    };
    let (diagnostics, _) = run_check(&root, &[], true, false, &flags);

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
        fail_on_untracked: false,
    };
    let (diagnostics, _) = run_check(&root, &[], false, false, &flags);

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
        fail_on_untracked: false,
    };
    // Fix should auto-correct the span
    let _ = run_check(&root, &[], true, false, &flags);
    // Re-check should be clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

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
        fail_on_untracked: false,
    };
    let (diagnostics, _) = run_check(&root, &[], false, false, &flags);

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
        fail_on_untracked: false,
    };
    // Fix should auto-correct the span via tree_path
    let _ = run_check(&root, &[], true, false, &flags);
    // Re-check should be clean
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_current = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Current));
    assert!(
        has_current,
        "expected Current after tree_path fix, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

/// Semantic drift: tree_path resolves the item to a new span, but the
/// content at that span has also changed (not just shifted).  `--fix`
/// should update the span to track the item, but NOT rewrite the hash,
/// so the spec remains stale for human review.
#[test]
fn semantic_drift_fix_preserves_stale() {
    let (_tmp, root) = fixture_in_tmp("semantic_drift");
    let flags = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
        fail_on_untracked: true,
    };

    // First pass with --fix: should update span via tree_path but leave
    // the hash stale because the content changed (x*2+1 → x*3+1).
    let (diags_fix, _) = run_check(&root, &[], true, false, &flags);

    let has_stale = diags_fix
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Stale));
    assert!(
        has_stale,
        "expected Stale diagnostic during --fix pass, got: {diags_fix:#?}"
    );

    // Second pass WITHOUT --fix: the span should have been corrected to
    // [4,6] but the hash should still be the OLD hash, so it remains Stale.
    let (diags_recheck, exit_code) = run_check(&root, &[], false, false, &flags);

    let still_stale = diags_recheck
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Stale));
    assert!(
        still_stale,
        "expected Stale on re-check (semantic drift not silently blessed), got: {diags_recheck:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

/// Semantic drift on an UNREVIEWED spec: tree_path resolves to a new span,
/// content has changed, but `reviewed` is false — no human judgment at stake.
/// `--fix` should update the span AND rehash (auto-rehash for unreviewed).
/// After fix, the spec should be current (not stale).
#[test]
fn semantic_drift_unreviewed_auto_rehash() {
    let (_tmp, root) = fixture_in_tmp("semantic_drift_unreviewed");
    let flags = CheckFlags {
        fail_on_stale: true,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
        fail_on_untracked: true,
    };

    // First pass with --fix: should update span via tree_path AND rehash
    // because the spec is unreviewed — no human judgment to protect.
    let (diags_fix, _) = run_check(&root, &[], true, false, &flags);

    let has_stale_fixed = diags_fix
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Stale) && d.fixed);
    assert!(
        has_stale_fixed,
        "expected a fixed Stale diagnostic during --fix pass, got: {diags_fix:#?}"
    );

    // Second pass WITHOUT --fix: the span should be corrected to [4,6]
    // and hash updated — spec should now be Current, not Stale.
    let (diags_recheck, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_current = diags_recheck
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Current));
    assert!(
        has_current,
        "expected Current on re-check (unreviewed spec auto-rehashed), got: {diags_recheck:#?}"
    );
    let still_stale = diags_recheck
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::Stale));
    assert!(
        !still_stale,
        "unreviewed spec should not remain stale after --fix, got: {diags_recheck:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

#[test]
fn req_changed() {
    let root = fixture_path("req_changed");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: true,
        fail_on_untracked: true,
    };
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_req_changed = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::ReqChanged { .. }));
    assert!(
        has_req_changed,
        "expected ReqChanged diagnostic, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

#[test]
fn req_cycle() {
    let root = fixture_path("req_cycle");
    let flags = default_flags();
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_cycle = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::RequirementCycle { .. }));
    assert!(
        has_cycle,
        "expected RequirementCycle diagnostic, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

/// `.liyiignore` excludes the `ignored/` directory.
/// The stale sidecar in `ignored/` should NOT produce diagnostics.
/// Only `visible.rs` should be checked.
#[test]
fn liyiignore() {
    let (_tmp, root) = fixture_in_tmp("liyiignore");
    let flags = default_flags();

    // Fix hashes for visible.rs first.
    let _ = run_check(&root, &[], true, false, &flags);

    // Re-check: only visible.rs should appear; the ignored stale sidecar
    // should produce no diagnostics at all.
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    // No diagnostic should reference anything in "ignored/".
    let has_hidden = diagnostics
        .iter()
        .any(|d| d.item_or_req == "hidden" || d.file.to_string_lossy().contains("ignored"));
    assert!(
        !has_hidden,
        "expected ignored directory to be excluded, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

#[test]
fn missing_related() {
    // Copy to tmp so check runs in isolation (no repo root detection issues)
    let (_tmp, root) = fixture_in_tmp("missing_related");
    // Use lenient flags - we only care about MissingRelatedEdge, not other issues
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: false,
    };

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_missing_related = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::MissingRelatedEdge { .. }));
    assert!(
        has_missing_related,
        "expected MissingRelatedEdge diagnostic, got: {diagnostics:#?}"
    );
    // MissingRelatedEdge is treated as an unconditional check failure,
    // so the exit code is CheckFailure even with all failure flags disabled.
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

#[test]
fn missing_related_pass() {
    // Copy to tmp so check runs in isolation (no repo root detection issues)
    let (_tmp, root) = fixture_in_tmp("missing_related_pass");
    let flags = default_flags();

    // Fix hashes first so we don't get Stale diagnostics
    let _ = run_check(&root, &[], true, false, &flags);

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    let has_missing_related = diagnostics
        .iter()
        .any(|d| matches!(d.kind, DiagnosticKind::MissingRelatedEdge { .. }));
    assert!(
        !has_missing_related,
        "expected no MissingRelatedEdge diagnostic when edge exists, got: {diagnostics:#?}"
    );
    assert_eq!(exit_code, LiyiExitCode::Clean);
}

// ---------------------------------------------------------------------------
// --prompt mode tests
// ---------------------------------------------------------------------------

#[test]
fn prompt_mixed_gaps() {
    let (_tmp, root) = fixture_in_tmp("prompt_output/mixed_gaps");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: true,
    };

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);
    let output = liyi::prompt::build_prompt_output(&diagnostics, exit_code, &root);

    assert_eq!(output.version, "0.1");
    assert_eq!(output.exit_code, 1);

    // Should have all three gap types.
    let types: Vec<&str> = output
        .items
        .iter()
        .map(|item| match item {
            liyi::prompt::PromptItem::MissingRequirementSpec { .. } => "missing_requirement_spec",
            liyi::prompt::PromptItem::MissingRelatedEdge { .. } => "missing_related_edge",
            liyi::prompt::PromptItem::ReqNoRelated { .. } => "req_no_related",
        })
        .collect();

    assert!(
        types.contains(&"missing_requirement_spec"),
        "expected missing_requirement_spec in prompt items, got: {types:?}"
    );
    assert!(
        types.contains(&"missing_related_edge"),
        "expected missing_related_edge in prompt items, got: {types:?}"
    );
    assert!(
        types.contains(&"req_no_related"),
        "expected req_no_related in prompt items, got: {types:?}"
    );

    // Verify requirement_text is populated for missing_requirement_spec.
    for item in &output.items {
        if let liyi::prompt::PromptItem::MissingRequirementSpec {
            requirement_text, ..
        } = item
        {
            assert!(
                requirement_text.is_some(),
                "expected requirement_text to be populated"
            );
        }
    }

    // Verify output serializes to valid JSON.
    let json = serde_json::to_string_pretty(&output).expect("failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("invalid JSON");
    assert_eq!(parsed["version"], "0.1");
    assert!(parsed["items"].is_array());
}

#[test]
fn prompt_clean() {
    let (_tmp, root) = fixture_in_tmp("prompt_output/clean");
    let flags = default_flags();

    // Fix hashes first.
    let _ = run_check(&root, &[], true, false, &flags);

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);
    let output = liyi::prompt::build_prompt_output(&diagnostics, exit_code, &root);

    assert_eq!(output.version, "0.1");
    assert!(
        output.items.is_empty(),
        "expected no items, got: {:?}",
        output.items
    );
    assert_eq!(output.exit_code, 0);
}

#[test]
fn prompt_errors_only() {
    let (_tmp, root) = fixture_in_tmp("prompt_output/errors_only");
    let flags = default_flags();

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);
    let output = liyi::prompt::build_prompt_output(&diagnostics, exit_code, &root);

    // Error-class diagnostics produce exit_code 2 but no coverage-gap items.
    assert!(
        output.items.is_empty(),
        "expected no items for error-only, got: {:?}",
        output.items
    );
    assert_eq!(output.exit_code, 2);
}

#[test]
fn prompt_multi_file() {
    let (_tmp, root) = fixture_in_tmp("prompt_output/multi_file");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: true,
    };

    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);
    let output = liyi::prompt::build_prompt_output(&diagnostics, exit_code, &root);

    assert_eq!(output.exit_code, 1);

    // Should have gaps from both files.
    let source_files: Vec<&str> = output
        .items
        .iter()
        .map(|item| match item {
            liyi::prompt::PromptItem::MissingRequirementSpec { source_file, .. } => {
                source_file.as_str()
            }
            liyi::prompt::PromptItem::MissingRelatedEdge { source_file, .. } => {
                source_file.as_str()
            }
            liyi::prompt::PromptItem::ReqNoRelated { source_file, .. } => source_file.as_str(),
        })
        .collect();

    assert!(
        source_files.contains(&"alpha.rs"),
        "expected gaps from alpha.rs, got: {source_files:?}"
    );
    assert!(
        source_files.contains(&"beta.rs"),
        "expected gaps from beta.rs, got: {source_files:?}"
    );
    assert!(
        output.items.len() >= 2,
        "expected at least 2 gap items across files"
    );
}

// ---------------------------------------------------------------------------
// =trivial sidecar sentinel tests
// ---------------------------------------------------------------------------

#[test]
fn trivial_sidecar_sentinel() {
    let (_tmp, root) = fixture_in_tmp("trivial_sidecar");
    let flags = CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: false,
    };

    // First pass: fix to fill in source_hash / source_anchor.
    let _ = run_check(&root, &[], true, false, &flags);

    // Second pass: check diagnostics.
    let (diagnostics, exit_code) = run_check(&root, &[], false, false, &flags);

    // get_count has intent "=trivial" with no source annotation → Trivial info
    let get_count_trivial = diagnostics
        .iter()
        .any(|d| d.item_or_req == "get_count" && matches!(d.kind, DiagnosticKind::Trivial));
    assert!(
        get_count_trivial,
        "expected Trivial diagnostic for get_count (=trivial intent), got: {diagnostics:#?}"
    );

    // get_label has both @liyi:trivial in source and =trivial in sidecar → two Trivial diagnostics, no error
    let get_label_trivials: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.item_or_req == "get_label" && matches!(d.kind, DiagnosticKind::Trivial))
        .collect();
    assert!(
        get_label_trivials.len() >= 2,
        "expected at least 2 Trivial diagnostics for get_label (source + sidecar), got {} in: {diagnostics:#?}",
        get_label_trivials.len()
    );

    // compute_total has @liyi:nontrivial in source + =trivial in sidecar → ConflictingTriviality error
    let has_conflict = diagnostics.iter().any(|d| {
        d.item_or_req == "compute_total" && matches!(d.kind, DiagnosticKind::ConflictingTriviality)
    });
    assert!(
        has_conflict,
        "expected ConflictingTriviality for compute_total, got: {diagnostics:#?}"
    );

    // ConflictingTriviality should cause check failure
    assert_eq!(exit_code, LiyiExitCode::CheckFailure);
}

// ---------------------------------------------------------------------------
// Init discovery tests
// ---------------------------------------------------------------------------

#[test]
fn init_discover_populates_specs() {
    let fixture = fixture_path("init_discover");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    let sidecar_path =
        liyi::init::init_sidecar(&tmp_source, false, true, 5).expect("init_sidecar should succeed");

    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    // Should have discovered multiple items
    assert!(
        sidecar.specs.len() >= 5,
        "expected at least 5 items (struct, impl, 2 methods, standalone fn), got {}",
        sidecar.specs.len()
    );

    // Check that each spec has the expected fields populated
    for spec in &sidecar.specs {
        if let liyi::sidecar::Spec::Item(item) = spec {
            assert!(!item.item.is_empty(), "item name should be non-empty");
            assert!(!item.tree_path.is_empty(), "tree_path should be populated");
            assert!(item.source_span[0] >= 1, "span start should be >= 1");
            assert!(
                item.source_span[1] >= item.source_span[0],
                "span end should be >= start"
            );
            assert!(
                item.intent.is_empty(),
                "intent should be empty (agent fills it)"
            );
            assert!(!item.reviewed, "reviewed should be false");
        }
    }

    // Verify specific items are present
    let names: Vec<String> = sidecar
        .specs
        .iter()
        .filter_map(|s| match s {
            liyi::sidecar::Spec::Item(i) => Some(i.item.clone()),
            _ => None,
        })
        .collect();

    assert!(
        names.contains(&"Money".to_string()),
        "should discover struct Money"
    );
    assert!(
        names.contains(&"Money::new".to_string()),
        "should discover Money::new"
    );
    assert!(
        names.contains(&"Money::add".to_string()),
        "should discover Money::add"
    );
    assert!(
        names.contains(&"standalone".to_string()),
        "should discover standalone fn"
    );
}

#[test]
fn init_no_discover_produces_empty_specs() {
    let fixture = fixture_path("init_discover");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    let sidecar_path = liyi::init::init_sidecar(&tmp_source, false, false, 5)
        .expect("init_sidecar should succeed");

    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    assert!(
        sidecar.specs.is_empty(),
        "should have empty specs with --no-discover, got {}",
        sidecar.specs.len()
    );
}

#[test]
fn init_discover_detects_doc_comments() {
    let fixture = fixture_path("init_discover");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    let sidecar_path =
        liyi::init::init_sidecar(&tmp_source, false, true, 5).expect("init_sidecar should succeed");

    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    // The struct Money has a doc comment (/// A monetary amount)
    let money_struct = sidecar.specs.iter().find_map(|s| match s {
        liyi::sidecar::Spec::Item(i) if i.tree_path == "struct::Money" => Some(i),
        _ => None,
    });
    assert!(money_struct.is_some(), "should find struct::Money");
    let hints = money_struct
        .unwrap()
        ._hints
        .as_ref()
        .expect("should have _hints");
    assert_eq!(
        hints.get("_has_doc"),
        Some(&serde_json::Value::Bool(true)),
        "struct Money should have _has_doc: true"
    );

    // standalone function has no doc comment
    let standalone = sidecar.specs.iter().find_map(|s| match s {
        liyi::sidecar::Spec::Item(i) if i.tree_path == "fn::standalone" => Some(i),
        _ => None,
    });
    assert!(standalone.is_some(), "should find fn::standalone");
    let hints = standalone
        .unwrap()
        ._hints
        .as_ref()
        .expect("should have _hints");
    assert_eq!(
        hints.get("_has_doc"),
        Some(&serde_json::Value::Bool(false)),
        "standalone should have _has_doc: false"
    );
}

#[test]
fn init_discover_body_lines_and_likely_trivial() {
    let fixture = fixture_path("init_discover");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    // Use threshold of 5
    liyi::init::init_sidecar(&tmp_source, false, true, 5).expect("init_sidecar should succeed");

    let sidecar_path = tmp_source.with_file_name("example.rs.liyi.jsonc");
    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    // fn standalone (lines 24-26, 3 lines, no doc) => _likely_trivial: true
    let standalone = sidecar
        .specs
        .iter()
        .find_map(|s| match s {
            liyi::sidecar::Spec::Item(i) if i.tree_path == "fn::standalone" => Some(i),
            _ => None,
        })
        .expect("should find fn::standalone");
    let hints = standalone._hints.as_ref().expect("should have _hints");
    assert_eq!(hints["_body_lines"], 3);
    assert_eq!(hints["_likely_trivial"], true);

    // struct Money has a doc comment => not _likely_trivial even though small
    let money = sidecar
        .specs
        .iter()
        .find_map(|s| match s {
            liyi::sidecar::Spec::Item(i) if i.tree_path == "struct::Money" => Some(i),
            _ => None,
        })
        .expect("should find struct::Money");
    let money_hints = money._hints.as_ref().expect("should have _hints");
    assert!(money_hints["_body_lines"].is_number());
    assert!(
        money_hints.get("_likely_trivial").is_none(),
        "struct Money has doc comment so should not be _likely_trivial"
    );

    // impl Money::add — multi-line function, not _likely_trivial with threshold 5
    let add_fn = sidecar
        .specs
        .iter()
        .find_map(|s| match s {
            liyi::sidecar::Spec::Item(i) if i.tree_path == "impl::Money::fn::add" => Some(i),
            _ => None,
        })
        .expect("should find impl::Money::fn::add");
    let add_hints = add_fn._hints.as_ref().expect("should have _hints");
    let add_lines = add_hints["_body_lines"].as_u64().unwrap();
    assert!(
        add_lines > 5,
        "fn add should have more than 5 lines, got {add_lines}"
    );
    assert!(
        add_hints.get("_likely_trivial").is_none(),
        "fn add should not be _likely_trivial with threshold 5"
    );
}

#[test]
fn init_discover_custom_trivial_threshold() {
    let fixture = fixture_path("init_discover");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    // With threshold 15, fn add (multi-line, no doc) should be _likely_trivial
    liyi::init::init_sidecar(&tmp_source, false, true, 15).expect("init_sidecar should succeed");

    let sidecar_path = tmp_source.with_file_name("example.rs.liyi.jsonc");
    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    let add_fn = sidecar
        .specs
        .iter()
        .find_map(|s| match s {
            liyi::sidecar::Spec::Item(i) if i.tree_path == "impl::Money::fn::add" => Some(i),
            _ => None,
        })
        .expect("should find impl::Money::fn::add");
    let hints = add_fn._hints.as_ref().expect("should have _hints");
    assert_eq!(hints["_likely_trivial"], true);
}

#[test]
fn check_fix_strips_hints() {
    let tmp = tempfile::TempDir::new().unwrap();
    let source_path = tmp.path().join("example.rs");
    fs::write(&source_path, "fn hello() -> i32 { 42 }\n").unwrap();

    // Create a sidecar with _hints
    liyi::init::init_sidecar(&source_path, false, true, 5).expect("init should succeed");

    let sidecar_path = source_path.with_file_name("example.rs.liyi.jsonc");
    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    // Verify hints are present before check --fix
    assert!(
        !sidecar.specs.is_empty(),
        "should have at least one discovered item"
    );
    match &sidecar.specs[0] {
        liyi::sidecar::Spec::Item(item) => {
            assert!(
                item._hints.is_some(),
                "_hints should be present before check --fix"
            );
        }
        _ => panic!("expected item spec"),
    }

    // Run check with fix=true
    let flags = liyi::diagnostics::CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: false,
    };
    liyi::check::run_check(tmp.path(), &[], true, false, &flags);

    // Re-read and verify _hints are stripped
    let after_content = fs::read_to_string(&sidecar_path).unwrap();
    let after =
        liyi::sidecar::parse_sidecar(&after_content).expect("sidecar should parse after fix");
    for spec in &after.specs {
        if let liyi::sidecar::Spec::Item(item) = spec {
            assert!(
                item._hints.is_none(),
                "_hints should be stripped after check --fix"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M10.5 — Combined scaffold golden test
// ---------------------------------------------------------------------------

/// End-to-end golden test for the full `liyi init` scaffold workflow.
///
/// Verifies that a multi-item source file produces a pre-populated sidecar
/// with correct `tree_path`, `source_span`, and `_hints` (doc comment,
/// line count, `_likely_trivial`).  Also verifies the sidecar round-trips
/// through `check --fix` (hints stripped, hashes filled).
#[test]
fn init_scaffold_combined() {
    let fixture = fixture_path("init_scaffold_combined");
    let source = fixture.join("example.rs");
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_source = tmp.path().join("example.rs");
    fs::copy(&source, &tmp_source).unwrap();

    // Phase 1: run init with discover (default threshold = 5)
    let sidecar_path = liyi::init::init_sidecar(&tmp_source, false, true, 5)
        .expect("init_sidecar should succeed");

    let content = fs::read_to_string(&sidecar_path).unwrap();
    let sidecar = liyi::sidecar::parse_sidecar(&content).expect("sidecar should parse");

    assert_eq!(sidecar.version, "0.1");
    assert_eq!(sidecar.source, "example.rs");

    // Collect items keyed by tree_path for deterministic assertion.
    let items: std::collections::HashMap<&str, &liyi::sidecar::ItemSpec> = sidecar
        .specs
        .iter()
        .filter_map(|s| match s {
            liyi::sidecar::Spec::Item(i) => Some((i.tree_path.as_str(), i)),
            _ => None,
        })
        .collect();

    // ---- Expected items table ----
    struct Expected {
        tp: &'static str,
        name: &'static str,
        span: [usize; 2],
        body: u64,
        has_doc: bool,
        trivial: bool,
    }
    let expected: &[Expected] = &[
        Expected { tp: "struct::Point",                name: "Point",             span: [2, 5],   body: 4,  has_doc: true,  trivial: false },
        Expected { tp: "struct::Internal",             name: "Internal",          span: [7, 7],   body: 1,  has_doc: false, trivial: true },
        Expected { tp: "impl::Point",                  name: "Point",             span: [9, 28],  body: 20, has_doc: false, trivial: false },
        Expected { tp: "impl::Point::fn::origin",      name: "Point::origin",     span: [11, 13], body: 3,  has_doc: true,  trivial: false },
        Expected { tp: "impl::Point::fn::distance_to", name: "Point::distance_to", span: [18, 22], body: 5, has_doc: true, trivial: false },
        Expected { tp: "impl::Point::fn::translate",   name: "Point::translate",  span: [24, 27], body: 4,  has_doc: false, trivial: true },
        Expected { tp: "fn::scale_all",                name: "scale_all",         span: [31, 36], body: 6,  has_doc: true,  trivial: false },
        Expected { tp: "fn::identity",                 name: "identity",          span: [38, 40], body: 3,  has_doc: false, trivial: true },
    ];

    assert_eq!(
        items.len(),
        expected.len(),
        "expected {} items, got {}: {:?}",
        expected.len(),
        items.len(),
        items.keys().collect::<Vec<_>>()
    );

    for e in expected {
        let item = items
            .get(e.tp)
            .unwrap_or_else(|| panic!("missing item with tree_path {}", e.tp));

        assert_eq!(item.item, e.name, "{}: display name mismatch", e.tp);
        assert_eq!(item.source_span, e.span, "{}: source_span mismatch", e.tp);
        assert!(!item.reviewed, "{}: reviewed should be false", e.tp);
        assert!(item.intent.is_empty(), "{}: intent should be empty", e.tp);
        assert!(item.source_hash.is_none(), "{}: source_hash should be None", e.tp);
        assert!(item.source_anchor.is_none(), "{}: source_anchor should be None", e.tp);

        let hints = item._hints.as_ref().unwrap_or_else(|| panic!("{}: missing _hints", e.tp));
        assert_eq!(hints["_body_lines"], e.body, "{}: _body_lines mismatch", e.tp);
        assert_eq!(hints["_has_doc"], e.has_doc, "{}: _has_doc mismatch", e.tp);
        if e.trivial {
            assert_eq!(
                hints["_likely_trivial"], true,
                "{}: expected _likely_trivial: true", e.tp
            );
        } else {
            assert!(
                hints.get("_likely_trivial").is_none(),
                "{}: should NOT have _likely_trivial", e.tp
            );
        }
    }

    // Phase 2: check --fix fills hashes and strips _hints.
    let flags = liyi::diagnostics::CheckFlags {
        fail_on_stale: false,
        fail_on_unreviewed: false,
        fail_on_req_changed: false,
        fail_on_untracked: false,
    };
    liyi::check::run_check(tmp.path(), &[], true, false, &flags);

    let after_content = fs::read_to_string(&sidecar_path).unwrap();
    let after = liyi::sidecar::parse_sidecar(&after_content).expect("sidecar should parse after fix");

    for spec in &after.specs {
        if let liyi::sidecar::Spec::Item(item) = spec {
            assert!(
                item._hints.is_none(),
                "{}: _hints should be stripped after check --fix",
                item.tree_path
            );
            assert!(
                item.source_hash.is_some(),
                "{}: source_hash should be filled after check --fix",
                item.tree_path
            );
            assert!(
                item.source_anchor.is_some(),
                "{}: source_anchor should be filled after check --fix",
                item.tree_path
            );
        }
    }

    // Phase 3: re-check should be clean (all current, no stale).
    let (diagnostics, exit_code) = liyi::check::run_check(tmp.path(), &[], false, false, &flags);
    let failures: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            !matches!(
                d.kind,
                liyi::diagnostics::DiagnosticKind::Current
                    | liyi::diagnostics::DiagnosticKind::Trivial
                    | liyi::diagnostics::DiagnosticKind::Ignored
                    | liyi::diagnostics::DiagnosticKind::Unreviewed
            )
        })
        .collect();
    assert!(failures.is_empty(), "unexpected diagnostics after fix: {failures:#?}");
    assert_eq!(exit_code, liyi::diagnostics::LiyiExitCode::Clean);
}
