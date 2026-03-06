// Proptest-based property tests for shift detection.
//
// Strategy: generate random file content, insert/delete lines at a random
// position, then verify that `detect_shift` correctly locates the original
// span at its new position and reports the expected delta.

use liyi::hashing::hash_span;
use liyi::shift::{ShiftResult, detect_shift, detect_shift_with_hint};
use proptest::prelude::*;

/// Generate a random source file as a Vec of lines.
fn arb_lines(min: usize, max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-zA-Z0-9_ ]{1,80}", min..=max)
}

/// Turn a Vec of lines into a single source string (newline-terminated).
fn lines_to_source(lines: &[String]) -> String {
    let mut s = String::new();
    for line in lines {
        s.push_str(line);
        s.push('\n');
    }
    s
}

proptest! {
    /// Inserting lines above the span shifts it down by the number of
    /// inserted lines.
    #[test]
    fn insert_lines_above_shifts_down(
        lines in arb_lines(10, 60),
        insert_count in 1usize..=20,
        insert_pos in 0usize..5,
    ) {
        let total = lines.len();
        // Pick a span inside the file (after the insert position).
        let span_start = (insert_pos + 1).min(total - 2) + 1; // 1-indexed, after insert_pos
        let span_end = (span_start + 1).min(total);

        let source = lines_to_source(&lines);
        let (hash, _) = hash_span(&source, [span_start, span_end]).unwrap();

        // Insert `insert_count` lines at `insert_pos` (0-indexed).
        let clamped_pos = insert_pos.min(total);
        let mut new_lines = lines.clone();
        for i in 0..insert_count {
            new_lines.insert(clamped_pos, format!("INSERTED_{i}"));
        }
        let new_source = lines_to_source(&new_lines);

        let result = detect_shift(&new_source, [span_start, span_end], &hash);
        let expected_delta = if clamped_pos < span_start {
            insert_count as i64
        } else {
            0i64
        };

        match result {
            ShiftResult::Shifted { delta, new_span } => {
                prop_assert_eq!(delta, expected_delta);
                prop_assert_eq!(new_span, [
                    (span_start as i64 + expected_delta) as usize,
                    (span_end as i64 + expected_delta) as usize,
                ]);
                // Verify the hash at the new position is correct.
                let (rehash, _) = hash_span(&new_source, new_span).unwrap();
                prop_assert!(rehash == hash);
            }
            ShiftResult::Stale => {
                prop_assert!(false, "expected Shifted but got Stale");
            }
        }
    }

    /// Deleting lines above the span shifts it up.
    #[test]
    fn delete_lines_above_shifts_up(
        lines in arb_lines(15, 60),
        delete_count in 1usize..=5,
        delete_pos in 0usize..5,
    ) {
        let total = lines.len();
        let clamped_delete = delete_count.min(total.saturating_sub(6));
        if clamped_delete == 0 { return Ok(()); }

        let clamped_pos = delete_pos.min(total.saturating_sub(clamped_delete + 3));

        // Span must be after the deleted region.
        let span_start = clamped_pos + clamped_delete + 1 + 1; // 1-indexed, after deleted lines
        let span_end = (span_start + 1).min(total);
        if span_start > total || span_end > total { return Ok(()); }

        let source = lines_to_source(&lines);
        let (hash, _) = hash_span(&source, [span_start, span_end]).unwrap();

        // Delete lines.
        let mut new_lines = lines.clone();
        new_lines.drain(clamped_pos..clamped_pos + clamped_delete);
        let new_source = lines_to_source(&new_lines);

        let result = detect_shift(&new_source, [span_start, span_end], &hash);
        let expected_delta = -(clamped_delete as i64);

        match result {
            ShiftResult::Shifted { delta, new_span } => {
                prop_assert_eq!(delta, expected_delta);
                let (rehash, _) = hash_span(&new_source, new_span).unwrap();
                prop_assert!(rehash == hash);
            }
            ShiftResult::Stale => {
                prop_assert!(false, "expected Shifted but got Stale");
            }
        }
    }

    /// When lines are modified within the span, detect_shift should return
    /// Stale (the content changed, so no offset can recover the hash).
    #[test]
    fn modify_span_content_returns_stale(
        lines in arb_lines(10, 40),
        modify_line in 0usize..10,
    ) {
        let total = lines.len();
        let span_start = (modify_line + 1).min(total - 1) ; // 1-indexed
        let span_end = (span_start + 1).min(total);

        let source = lines_to_source(&lines);
        let (hash, _) = hash_span(&source, [span_start, span_end]).unwrap();

        // Modify the content at span_start (0-indexed = span_start - 1).
        let mut new_lines = lines.clone();
        let idx = span_start - 1;
        new_lines[idx] = format!("TOTALLY_DIFFERENT_CONTENT_{modify_line}");
        let new_source = lines_to_source(&new_lines);

        // Only if the hash actually changed should we expect Stale.
        let (new_hash, _) = hash_span(&new_source, [span_start, span_end]).unwrap();
        if new_hash == hash {
            return Ok(()); // Coincidental collision — skip.
        }

        let result = detect_shift(&new_source, [span_start, span_end], &hash);
        // The modified line could coincidentally hash-match at a different offset.
        // We just verify the *reported* position has the correct hash.
        match result {
            ShiftResult::Shifted { new_span, .. } => {
                let (rehash, _) = hash_span(&new_source, new_span).unwrap();
                prop_assert!(rehash == hash,
                    "shifted to a position that doesn't match the expected hash");
            }
            ShiftResult::Stale => {
                // Expected — content is gone and nothing else matches.
            }
        }
    }

    /// The hint shortcut must agree with the full scan when the hint is
    /// correct, and still find the right answer when the hint is wrong.
    #[test]
    fn hint_agrees_with_full_scan(
        lines in arb_lines(15, 60),
        insert_count in 1usize..=10,
        insert_pos in 0usize..5,
        hint in -20i64..=20,
    ) {
        let total = lines.len();
        let span_start = (insert_pos + 2).min(total - 1) + 1;
        let span_end = (span_start + 1).min(total);
        if span_start > total || span_end > total { return Ok(()); }

        let source = lines_to_source(&lines);
        let (hash, _) = hash_span(&source, [span_start, span_end]).unwrap();

        let clamped_pos = insert_pos.min(total);
        let mut new_lines = lines.clone();
        for i in 0..insert_count {
            new_lines.insert(clamped_pos, format!("INS_{i}"));
        }
        let new_source = lines_to_source(&new_lines);

        let full = detect_shift(&new_source, [span_start, span_end], &hash);
        let hinted = detect_shift_with_hint(&new_source, [span_start, span_end], &hash, hint);

        // Both should find the same span (hinted may find it faster but
        // the result must be equivalent or better).
        match (&full, &hinted) {
            (ShiftResult::Shifted { new_span: s1, .. },
             ShiftResult::Shifted { new_span: s2, .. }) => {
                // Both found a valid position — verify hashes match at both.
                let (h1, _) = hash_span(&new_source, *s1).unwrap();
                let (h2, _) = hash_span(&new_source, *s2).unwrap();
                prop_assert!(h1 == hash, "full scan hash mismatch");
                prop_assert!(h2 == hash, "hinted hash mismatch");
            }
            (ShiftResult::Stale, ShiftResult::Stale) => {
                // Both agree it's gone — fine.
            }
            (ShiftResult::Stale, ShiftResult::Shifted { .. }) => {
                // Hint found something the scan missed — this can happen
                // if hint_delta > WINDOW.
            }
            (ShiftResult::Shifted { .. }, ShiftResult::Stale) => {
                // Scan found it but hint didn't — shouldn't happen since
                // hint falls back to full scan.
                prop_assert!(false, "full scan found it but hint version returned Stale");
            }
        }
    }
}
