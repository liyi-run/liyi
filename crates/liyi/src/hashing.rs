use sha2::{Digest, Sha256};
use std::fmt;

/// Errors produced when a source span is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanError {
    /// Span end exceeds the number of lines in the file.
    PastEof { end: usize, total: usize },
    /// Span start is greater than span end.
    Inverted { start: usize, end: usize },
    /// Span covers zero lines (start == 0 or end == 0).
    Empty,
}

impl fmt::Display for SpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PastEof { end, total } => {
                write!(f, "span end {end} exceeds file length {total}")
            }
            Self::Inverted { start, end } => {
                write!(f, "span start {start} > end {end}")
            }
            Self::Empty => write!(f, "span is empty (zero index)"),
        }
    }
}

impl std::error::Error for SpanError {}

/// Hash the source lines in `span` and return `("sha256:{hex}", anchor)`.
///
/// `span` is a 1-indexed, inclusive `[start, end]` interval.
/// Line endings are normalized to LF before hashing.
/// The anchor is the literal text of the first line (after CR stripping, untrimmed).
// @liyi:related source-span-semantics
pub fn hash_span(source_content: &str, span: [usize; 2]) -> Result<(String, String), SpanError> {
    let [start, end] = span;
    if start == 0 || end == 0 {
        return Err(SpanError::Empty);
    }
    if start > end {
        return Err(SpanError::Inverted { start, end });
    }

    let lines: Vec<&str> = source_content.lines().collect();
    let total = lines.len();
    if end > total {
        return Err(SpanError::PastEof { end, total });
    }

    let selected = &lines[start - 1..end];
    let joined = selected.join("\n");

    let hash = Sha256::digest(joined.as_bytes());
    let hex = format!("sha256:{hash:x}");
    let anchor = selected[0].to_owned();

    Ok((hex, anchor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_hash_and_anchor() {
        let src = "line one\nline two\nline three\n";
        let (hash, anchor) = hash_span(src, [1, 2]).unwrap();
        assert!(hash.starts_with("sha256:"));
        assert_eq!(anchor, "line one");
    }

    #[test]
    fn normalizes_crlf() {
        let lf = "a\nb\n";
        let crlf = "a\r\nb\r\n";
        let h1 = hash_span(lf, [1, 2]).unwrap().0;
        let h2 = hash_span(crlf, [1, 2]).unwrap().0;
        assert_eq!(h1, h2);
    }

    #[test]
    fn error_past_eof() {
        let src = "one\ntwo\n";
        assert_eq!(
            hash_span(src, [1, 5]),
            Err(SpanError::PastEof { end: 5, total: 2 })
        );
    }

    #[test]
    fn error_inverted() {
        assert_eq!(
            hash_span("a\nb\n", [3, 1]),
            Err(SpanError::Inverted { start: 3, end: 1 })
        );
    }

    #[test]
    fn error_empty() {
        assert_eq!(hash_span("a\n", [0, 1]), Err(SpanError::Empty));
    }
}
