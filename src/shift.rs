use crate::hashing::hash_span;

/// Result of attempting to relocate a shifted span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShiftResult {
    /// The span was found at a new position.
    Shifted { delta: i64, new_span: [usize; 2] },
    /// The span could not be relocated within the search window.
    Stale,
}

/// Maximum search distance (lines) in each direction.
const WINDOW: i64 = 100;

/// Try to relocate a span whose hash no longer matches.
///
/// Searches offsets −100..+100 (skipping 0) in small-delta-first order
/// (1, −1, 2, −2, …) and returns the first candidate whose hash equals
/// `expected_hash`.
pub fn detect_shift(
    source_content: &str,
    span: [usize; 2],
    expected_hash: &str,
) -> ShiftResult {
    try_offset(source_content, span, expected_hash, 0)
        .unwrap_or_else(|| scan(source_content, span, expected_hash))
}

/// Like [`detect_shift`], but tries `hint_delta` before the full scan.
///
/// Useful when multiple spans in the same file likely shifted by the same
/// amount (e.g. a block of lines was inserted above them all).
pub fn detect_shift_with_hint(
    source_content: &str,
    span: [usize; 2],
    expected_hash: &str,
    hint_delta: i64,
) -> ShiftResult {
    if hint_delta != 0 {
        if let Some(r) = try_offset(source_content, span, expected_hash, hint_delta) {
            return r;
        }
    }
    scan(source_content, span, expected_hash)
}

/// Full scan in small-delta-first order: 1, −1, 2, −2, …
fn scan(source_content: &str, span: [usize; 2], expected_hash: &str) -> ShiftResult {
    for abs_d in 1..=WINDOW {
        for sign in &[1i64, -1] {
            let d = abs_d * sign;
            if let Some(r) = try_offset(source_content, span, expected_hash, d) {
                return r;
            }
        }
    }
    ShiftResult::Stale
}

/// Check a single offset. Returns `Some(Shifted{…})` on hash match.
fn try_offset(
    source_content: &str,
    span: [usize; 2],
    expected_hash: &str,
    delta: i64,
) -> Option<ShiftResult> {
    let cs = (span[0] as i64) + delta;
    let ce = (span[1] as i64) + delta;
    if cs < 1 || ce < 1 {
        return None;
    }
    let candidate = [cs as usize, ce as usize];
    let (hash, _anchor) = hash_span(source_content, candidate).ok()?;
    if hash == expected_hash {
        Some(ShiftResult::Shifted {
            delta,
            new_span: candidate,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::hash_span;

    fn make_source(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    #[test]
    fn exact_match_returns_shifted_zero() {
        let src = make_source(10);
        let (hash, _) = hash_span(&src, [3, 5]).unwrap();
        // delta-0 means the span hasn't moved — detect_shift still reports it
        // because try_offset with 0 is checked first.
        assert_eq!(
            detect_shift(&src, [3, 5], &hash),
            ShiftResult::Shifted {
                delta: 0,
                new_span: [3, 5],
            }
        );
    }

    #[test]
    fn positive_shift() {
        // Original content at lines 3-5, then we prepend 4 blank lines.
        let original = make_source(10);
        let (hash, _) = hash_span(&original, [3, 5]).unwrap();

        let mut shifted = "blank\nblank\nblank\nblank\n".to_string();
        for line in original.lines() {
            shifted.push_str(line);
            shifted.push('\n');
        }

        // The old span [3,5] no longer matches; content is now at [7,9].
        let result = detect_shift(&shifted, [3, 5], &hash);
        assert_eq!(
            result,
            ShiftResult::Shifted {
                delta: 4,
                new_span: [7, 9],
            }
        );
    }

    #[test]
    fn negative_shift() {
        let original = make_source(20);
        let (hash, _) = hash_span(&original, [10, 12]).unwrap();

        // Remove first 3 lines → content shifts up by 3.
        let trimmed: String = original.lines().skip(3).map(|l| format!("{l}\n")).collect();

        let result = detect_shift(&trimmed, [10, 12], &hash);
        assert_eq!(
            result,
            ShiftResult::Shifted {
                delta: -3,
                new_span: [7, 9],
            }
        );
    }

    #[test]
    fn stale_when_content_gone() {
        let src = make_source(10);
        let result = detect_shift(&src, [2, 4], "sha256:0000000000000000");
        assert_eq!(result, ShiftResult::Stale);
    }

    #[test]
    fn hint_shortcut() {
        let original = make_source(20);
        let (hash, _) = hash_span(&original, [5, 7]).unwrap();

        // Shift content down by 10 lines.
        let mut shifted = String::new();
        for _ in 0..10 {
            shifted.push_str("pad\n");
        }
        for line in original.lines() {
            shifted.push_str(line);
            shifted.push('\n');
        }

        let result = detect_shift_with_hint(&shifted, [5, 7], &hash, 10);
        assert_eq!(
            result,
            ShiftResult::Shifted {
                delta: 10,
                new_span: [15, 17],
            }
        );
    }

    #[test]
    fn hint_wrong_falls_back() {
        let original = make_source(20);
        let (hash, _) = hash_span(&original, [5, 7]).unwrap();

        let mut shifted = "pad\npad\n".to_string();
        for line in original.lines() {
            shifted.push_str(line);
            shifted.push('\n');
        }

        // Hint is wrong (10), but fallback scan finds delta=2.
        let result = detect_shift_with_hint(&shifted, [5, 7], &hash, 10);
        assert_eq!(
            result,
            ShiftResult::Shifted {
                delta: 2,
                new_span: [7, 9],
            }
        );
    }
}
