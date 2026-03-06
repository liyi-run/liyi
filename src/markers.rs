/// Source-file marker scanner with full-width normalization and multilingual aliases.

/// A discovered marker in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMarker {
    Module { line: usize },
    Requirement { name: String, line: usize },
    Related { name: String, line: usize },
    Intent { prose: Option<String>, is_doc: bool, line: usize },
    Trivial { line: usize },
    Ignore { reason: Option<String>, line: usize },
    Nontrivial { line: usize },
}

/// Replace full-width punctuation with half-width equivalents.
pub fn normalize_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\u{FF20}' => out.push('@'),
            '\u{FF1A}' => out.push(':'),
            '\u{FF08}' => out.push('('),
            '\u{FF09}' => out.push(')'),
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Alias table — maps every accepted marker string to its canonical form.
// ---------------------------------------------------------------------------

/// Canonical marker keywords (without the leading `@`).
const CANON_IGNORE: &str = "@liyi:ignore";
const CANON_TRIVIAL: &str = "@liyi:trivial";
const CANON_NONTRIVIAL: &str = "@liyi:nontrivial";
const CANON_MODULE: &str = "@liyi:module";
const CANON_REQUIREMENT: &str = "@liyi:requirement";
const CANON_RELATED: &str = "@liyi:related";
const CANON_INTENT: &str = "@liyi:intent";

/// (alias, canonical) pairs.  Order does not matter.
const ALIAS_TABLE: &[(&str, &str)] = &[
    // ignore
    (CANON_IGNORE, CANON_IGNORE),
    ("@立意:忽略", CANON_IGNORE),
    ("@liyi:ignorar", CANON_IGNORE),
    ("@立意:無視", CANON_IGNORE),
    ("@liyi:ignorer", CANON_IGNORE),
    ("@립의:무시", CANON_IGNORE),
    // trivial
    (CANON_TRIVIAL, CANON_TRIVIAL),
    ("@立意:显然", CANON_TRIVIAL),
    ("@立意:自明", CANON_TRIVIAL),
    ("@립의:자명", CANON_TRIVIAL),
    // nontrivial
    (CANON_NONTRIVIAL, CANON_NONTRIVIAL),
    ("@立意:并非显然", CANON_NONTRIVIAL),
    ("@liyi:notrivial", CANON_NONTRIVIAL),
    ("@立意:非自明", CANON_NONTRIVIAL),
    ("@liyi:nãotrivial", CANON_NONTRIVIAL),
    ("@립의:비자명", CANON_NONTRIVIAL),
    // module
    (CANON_MODULE, CANON_MODULE),
    ("@立意:模块", CANON_MODULE),
    ("@liyi:módulo", CANON_MODULE),
    ("@立意:モジュール", CANON_MODULE),
    ("@립의:모듈", CANON_MODULE),
    // requirement
    (CANON_REQUIREMENT, CANON_REQUIREMENT),
    ("@立意:需求", CANON_REQUIREMENT),
    ("@liyi:requisito", CANON_REQUIREMENT),
    ("@立意:要件", CANON_REQUIREMENT),
    ("@liyi:exigence", CANON_REQUIREMENT),
    ("@립의:요건", CANON_REQUIREMENT),
    // related
    (CANON_RELATED, CANON_RELATED),
    ("@立意:有关", CANON_RELATED),
    ("@liyi:relacionado", CANON_RELATED),
    ("@立意:関連", CANON_RELATED),
    ("@liyi:lié", CANON_RELATED),
    ("@립의:관련", CANON_RELATED),
    // intent
    (CANON_INTENT, CANON_INTENT),
    ("@立意:意图", CANON_INTENT),
    ("@liyi:intención", CANON_INTENT),
    ("@立意:意図", CANON_INTENT),
    ("@liyi:intention", CANON_INTENT),
    ("@립의:의도", CANON_INTENT),
    ("@liyi:intenção", CANON_INTENT),
];

/// Try to find a known marker at any position in `normalized`.
/// Returns `(canonical, byte-offset past the matched alias)` on success.
fn find_marker(normalized: &str) -> Option<(&'static str, usize)> {
    for &(alias, canon) in ALIAS_TABLE {
        if let Some(pos) = normalized.find(alias) {
            return Some((canon, pos + alias.len()));
        }
    }
    None
}

/// Extract a name from the remainder after a keyword.
/// Rules: if first non-WS char is `(`, take everything up to matching `)`;
/// otherwise take the first whitespace-delimited token.
fn extract_name(rest: &str) -> Option<String> {
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('(') {
        let inner = &trimmed[1..];
        let end = inner.find(')')?;
        let name = inner[..end].trim();
        if name.is_empty() { None } else { Some(name.to_string()) }
    } else {
        let token = trimmed.split_whitespace().next()?;
        Some(token.to_string())
    }
}

/// Scan all lines of `content` and return discovered `@liyi:*` markers.
/// Line numbers are 1-indexed.
pub fn scan_markers(content: &str) -> Vec<SourceMarker> {
    let mut markers = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let normalized = normalize_line(raw_line);

        let (canon, after) = match find_marker(&normalized) {
            Some(pair) => pair,
            None => continue,
        };

        let rest = &normalized[after..];

        match canon {
            CANON_MODULE => markers.push(SourceMarker::Module { line: line_num }),
            CANON_TRIVIAL => markers.push(SourceMarker::Trivial { line: line_num }),
            CANON_NONTRIVIAL => markers.push(SourceMarker::Nontrivial { line: line_num }),
            CANON_IGNORE => {
                let reason = {
                    let t = rest.trim();
                    if t.is_empty() { None } else { Some(t.to_string()) }
                };
                markers.push(SourceMarker::Ignore { reason, line: line_num });
            }
            CANON_REQUIREMENT => {
                if let Some(name) = extract_name(rest) {
                    markers.push(SourceMarker::Requirement { name, line: line_num });
                }
            }
            CANON_RELATED => {
                if let Some(name) = extract_name(rest) {
                    markers.push(SourceMarker::Related { name, line: line_num });
                }
            }
            CANON_INTENT => {
                let trimmed = rest.trim();
                if trimmed == "=doc" || trimmed == "=文档" {
                    markers.push(SourceMarker::Intent {
                        prose: None,
                        is_doc: true,
                        line: line_num,
                    });
                } else {
                    let prose = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
                    markers.push(SourceMarker::Intent {
                        prose,
                        is_doc: false,
                        line: line_num,
                    });
                }
            }
            _ => {}
        }
    }

    markers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fullwidth() {
        assert_eq!(normalize_line("＠立意：忽略"), "@立意:忽略");
        assert_eq!(normalize_line("＠liyi：intent（x）"), "@liyi:intent(x)");
    }

    #[test]
    fn scan_module() {
        let m = scan_markers("// @liyi:module\n");
        assert_eq!(m.len(), 1);
        assert!(matches!(&m[0], SourceMarker::Module { line: 1 }));
    }

    #[test]
    fn scan_trivial_and_nontrivial() {
        let m = scan_markers("x\n// @liyi:trivial\ny\n// @liyi:nontrivial\n");
        assert_eq!(m.len(), 2);
        assert!(matches!(&m[0], SourceMarker::Trivial { line: 2 }));
        assert!(matches!(&m[1], SourceMarker::Nontrivial { line: 4 }));
    }

    #[test]
    fn scan_ignore_with_reason() {
        let m = scan_markers("// @liyi:ignore generated code\n");
        assert!(matches!(&m[0], SourceMarker::Ignore { reason: Some(r), line: 1 } if r == "generated code"));
    }

    #[test]
    fn scan_requirement_paren() {
        let m = scan_markers("// @liyi:requirement(currency-match) ...\n");
        assert!(matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "currency-match"));
    }

    #[test]
    fn scan_requirement_space() {
        let m = scan_markers("// @liyi:requirement currency-match\n");
        assert!(matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "currency-match"));
    }

    #[test]
    fn scan_related() {
        let m = scan_markers("// @liyi:related some_req\n");
        assert!(matches!(&m[0], SourceMarker::Related { name, line: 1 } if name == "some_req"));
    }

    #[test]
    fn scan_intent_doc() {
        let m = scan_markers("// @liyi:intent =doc\n");
        assert!(matches!(&m[0], SourceMarker::Intent { prose: None, is_doc: true, line: 1 }));
    }

    #[test]
    fn scan_intent_doc_chinese() {
        let m = scan_markers("// @liyi:intent =文档\n");
        assert!(matches!(&m[0], SourceMarker::Intent { prose: None, is_doc: true, line: 1 }));
    }

    #[test]
    fn scan_intent_prose() {
        let m = scan_markers("// @liyi:intent Must reject negative amounts\n");
        assert!(matches!(&m[0], SourceMarker::Intent { prose: Some(p), is_doc: false, line: 1 } if p == "Must reject negative amounts"));
    }

    #[test]
    fn scan_alias_chinese() {
        let m = scan_markers("// @立意:忽略\n// @立意:模块\n");
        assert_eq!(m.len(), 2);
        assert!(matches!(&m[0], SourceMarker::Ignore { .. }));
        assert!(matches!(&m[1], SourceMarker::Module { .. }));
    }

    #[test]
    fn scan_fullwidth_normalization() {
        let m = scan_markers("// ＠立意：忽略\n");
        assert_eq!(m.len(), 1);
        assert!(matches!(&m[0], SourceMarker::Ignore { reason: None, line: 1 }));
    }
}
