/// Source-file marker scanner with full-width normalization and multilingual aliases.
///
/// A discovered marker in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMarker {
    Module {
        line: usize,
    },
    Requirement {
        name: String,
        line: usize,
    },
    Related {
        name: String,
        line: usize,
    },
    Intent {
        prose: Option<String>,
        is_doc: bool,
        line: usize,
    },
    Trivial {
        line: usize,
    },
    Ignore {
        reason: Option<String>,
        line: usize,
    },
    Nontrivial {
        line: usize,
    },
}

/// Replace full-width punctuation with half-width equivalents.
// @liyi:related marker-normalization
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
///
/// NOTE: The `@` is escaped as `\x40` to avoid the linter's own marker
/// scanner matching these string constants — the classic quine-escape problem.
// @liyi:requirement(quine-escape)
// The linter's own source files must not contain literal `@liyi:*` (or any
// alias) marker strings inside string constants, format strings, or test
// data.  Use `\x40` (Rust) or `\u0040` (JSON) to spell the `@` so the
// scanner does not self-trigger.  In documentation files, the scanner
// additionally suppresses markers inside fenced code blocks, inline
// backtick spans, and markers preceded by quotation marks — see
// *Self-hosting and the quine problem* in the design doc.  Actual marker
// comments (like this one) are the sole exception.
const CANON_IGNORE: &str = "\x40liyi:ignore";
const CANON_TRIVIAL: &str = "\x40liyi:trivial";
const CANON_NONTRIVIAL: &str = "\x40liyi:nontrivial";
const CANON_MODULE: &str = "\x40liyi:module";
const CANON_REQUIREMENT: &str = "\x40liyi:requirement";
const CANON_RELATED: &str = "\x40liyi:related";
const CANON_INTENT: &str = "\x40liyi:intent";

/// (alias, canonical) pairs.  Order does not matter.
// @liyi:related marker-normalization
const ALIAS_TABLE: &[(&str, &str)] = &[
    // ignore
    (CANON_IGNORE, CANON_IGNORE),
    ("\x40立意:忽略", CANON_IGNORE),
    ("\x40liyi:ignorar", CANON_IGNORE),
    ("\x40立意:無視", CANON_IGNORE),
    ("\x40liyi:ignorer", CANON_IGNORE),
    ("\x40립의:무시", CANON_IGNORE),
    // trivial
    (CANON_TRIVIAL, CANON_TRIVIAL),
    ("\x40立意:显然", CANON_TRIVIAL),
    ("\x40立意:自明", CANON_TRIVIAL),
    ("\x40립의:자명", CANON_TRIVIAL),
    // nontrivial
    (CANON_NONTRIVIAL, CANON_NONTRIVIAL),
    ("\x40立意:并非显然", CANON_NONTRIVIAL),
    ("\x40liyi:notrivial", CANON_NONTRIVIAL),
    ("\x40立意:非自明", CANON_NONTRIVIAL),
    ("\x40liyi:nãotrivial", CANON_NONTRIVIAL),
    ("\x40립의:비자명", CANON_NONTRIVIAL),
    // module
    (CANON_MODULE, CANON_MODULE),
    ("\x40立意:模块", CANON_MODULE),
    ("\x40liyi:módulo", CANON_MODULE),
    ("\x40立意:モジュール", CANON_MODULE),
    ("\x40립의:모듈", CANON_MODULE),
    // requirement
    (CANON_REQUIREMENT, CANON_REQUIREMENT),
    ("\x40立意:需求", CANON_REQUIREMENT),
    ("\x40liyi:requisito", CANON_REQUIREMENT),
    ("\x40立意:要件", CANON_REQUIREMENT),
    ("\x40liyi:exigence", CANON_REQUIREMENT),
    ("\x40립의:요건", CANON_REQUIREMENT),
    // related
    (CANON_RELATED, CANON_RELATED),
    ("\x40立意:有关", CANON_RELATED),
    ("\x40liyi:relacionado", CANON_RELATED),
    ("\x40立意:関連", CANON_RELATED),
    ("\x40liyi:lié", CANON_RELATED),
    ("\x40립의:관련", CANON_RELATED),
    // intent
    (CANON_INTENT, CANON_INTENT),
    ("\x40立意:意图", CANON_INTENT),
    ("\x40liyi:intención", CANON_INTENT),
    ("\x40立意:意図", CANON_INTENT),
    ("\x40liyi:intention", CANON_INTENT),
    ("\x40립의:의도", CANON_INTENT),
    ("\x40liyi:intenção", CANON_INTENT),
];

/// Try to find a known marker at any position in `normalized`.
/// Returns `(canonical, byte-offset of match start, byte-offset past the matched alias)` on success.
// @liyi:related marker-normalization
fn find_marker(normalized: &str) -> Option<(&'static str, usize, usize)> {
    for &(alias, canon) in ALIAS_TABLE {
        if let Some(pos) = normalized.find(alias) {
            return Some((canon, pos, pos + alias.len()));
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
    if let Some(inner) = trimmed.strip_prefix('(') {
        let end = inner.find(')')?;
        let name = inner[..end].trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else {
        let token = trimmed.split_whitespace().next()?;
        Some(token.to_string())
    }
}

// ---------------------------------------------------------------------------
// NL-quoting quine suppression
// ---------------------------------------------------------------------------

/// Characters that, when immediately preceding the `@` of a marker, cause
/// the marker to be rejected as a documentary mention rather than a real
/// directive.  Covers ASCII quotes, typographic quotes, CJK brackets, and
/// guillemets.
const QUOTE_CHARS: &[char] = &[
    '\'',       // U+0027 apostrophe
    '"',        // U+0022 quotation mark
    '`',        // U+0060 grave accent (backtick) — defense-in-depth with span check
    '\u{2018}', // ' left single quotation mark
    '\u{2019}', // ' right single quotation mark
    '\u{201C}', // " left double quotation mark
    '\u{201D}', // " right double quotation mark
    '\u{300C}', // 「 left corner bracket
    '\u{300D}', // 」 right corner bracket
    '\u{00AB}', // « left guillemet
    '\u{00BB}', // » right guillemet
];

/// Returns true if `byte_pos` falls inside an inline backtick span.
/// Determined by counting backtick characters before the position —
/// an odd count means we are inside a code span.
fn is_in_inline_code(line: &str, byte_pos: usize) -> bool {
    let mut count = 0u32;
    for (i, ch) in line.char_indices() {
        if i >= byte_pos {
            break;
        }
        if ch == '`' {
            count += 1;
        }
    }
    !count.is_multiple_of(2)
}

/// Returns true if the character immediately before `byte_pos` in `line`
/// is a quotation mark (ASCII, typographic, CJK, or guillemet).
fn preceded_by_quote(line: &str, byte_pos: usize) -> bool {
    // Find the last char before byte_pos.
    let prefix = &line[..byte_pos];
    match prefix.chars().next_back() {
        Some(ch) => QUOTE_CHARS.contains(&ch),
        None => false,
    }
}

/// Returns true if a trimmed line opens or closes a fenced code block.
// @liyi:related markdown-fenced-block-skip
fn is_fence_delimiter(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Scan all lines of `content` and return discovered `@liyi:*` markers.
/// Line numbers are 1-indexed.
///
/// Markers are suppressed (not returned) when they appear inside fenced
/// code blocks, inside inline backtick spans, or immediately after a
/// quotation-mark character.  See *Self-hosting and the quine problem*
/// in the design doc.
// @liyi:related markdown-fenced-block-skip
// @liyi:related quine-escape-in-source
pub fn scan_markers(content: &str) -> Vec<SourceMarker> {
    let mut markers = Vec::new();
    let mut in_fenced_block = false;

    for (idx, raw_line) in content.lines().enumerate() {
        let line_num = idx + 1;

        // Fenced code block toggle (``` or ~~~).
        if is_fence_delimiter(raw_line) {
            in_fenced_block = !in_fenced_block;
            continue;
        }
        if in_fenced_block {
            continue;
        }

        let normalized = normalize_line(raw_line);

        let (canon, match_start, after) = match find_marker(&normalized) {
            Some(triple) => triple,
            None => continue,
        };

        // NL-quoting suppression: inline backtick span.
        if is_in_inline_code(&normalized, match_start) {
            continue;
        }

        // NL-quoting suppression: preceding quote character.
        if preceded_by_quote(&normalized, match_start) {
            continue;
        }

        let rest = &normalized[after..];

        match canon {
            CANON_MODULE => markers.push(SourceMarker::Module { line: line_num }),
            CANON_TRIVIAL => markers.push(SourceMarker::Trivial { line: line_num }),
            CANON_NONTRIVIAL => markers.push(SourceMarker::Nontrivial { line: line_num }),
            CANON_IGNORE => {
                let reason = {
                    let t = rest.trim();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t.to_string())
                    }
                };
                markers.push(SourceMarker::Ignore {
                    reason,
                    line: line_num,
                });
            }
            CANON_REQUIREMENT => {
                if let Some(name) = extract_name(rest) {
                    markers.push(SourceMarker::Requirement {
                        name,
                        line: line_num,
                    });
                }
            }
            CANON_RELATED => {
                if let Some(name) = extract_name(rest) {
                    markers.push(SourceMarker::Related {
                        name,
                        line: line_num,
                    });
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
                    let prose = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
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
        assert_eq!(normalize_line("\u{FF20}立意\u{FF1A}忽略"), "\x40立意:忽略");
        assert_eq!(
            normalize_line("\u{FF20}liyi\u{FF1A}intent\u{FF08}x\u{FF09}"),
            "\x40liyi:intent(x)"
        );
    }

    #[test]
    fn scan_module() {
        let m = scan_markers("// \x40liyi:module\n");
        assert_eq!(m.len(), 1);
        assert!(matches!(&m[0], SourceMarker::Module { line: 1 }));
    }

    #[test]
    fn scan_trivial_and_nontrivial() {
        let m = scan_markers("x\n// \x40liyi:trivial\ny\n// \x40liyi:nontrivial\n");
        assert_eq!(m.len(), 2);
        assert!(matches!(&m[0], SourceMarker::Trivial { line: 2 }));
        assert!(matches!(&m[1], SourceMarker::Nontrivial { line: 4 }));
    }

    #[test]
    fn scan_ignore_with_reason() {
        let m = scan_markers("// \x40liyi:ignore generated code\n");
        assert!(
            matches!(&m[0], SourceMarker::Ignore { reason: Some(r), line: 1 } if r == "generated code")
        );
    }

    #[test]
    fn scan_requirement_paren() {
        let m = scan_markers("// \x40liyi:requirement(currency-match) ...\n");
        assert!(
            matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "currency-match")
        );
    }

    #[test]
    fn scan_requirement_space() {
        let m = scan_markers("// \x40liyi:requirement currency-match\n");
        assert!(
            matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "currency-match")
        );
    }

    #[test]
    fn scan_related() {
        let m = scan_markers("// \x40liyi:related some_req\n");
        assert!(matches!(&m[0], SourceMarker::Related { name, line: 1 } if name == "some_req"));
    }

    #[test]
    fn scan_intent_doc() {
        let m = scan_markers("// \x40liyi:intent =doc\n");
        assert!(matches!(
            &m[0],
            SourceMarker::Intent {
                prose: None,
                is_doc: true,
                line: 1
            }
        ));
    }

    #[test]
    fn scan_intent_doc_chinese() {
        let m = scan_markers("// \x40liyi:intent =文档\n");
        assert!(matches!(
            &m[0],
            SourceMarker::Intent {
                prose: None,
                is_doc: true,
                line: 1
            }
        ));
    }

    #[test]
    fn scan_intent_prose() {
        let m = scan_markers("// \x40liyi:intent Must reject negative amounts\n");
        assert!(
            matches!(&m[0], SourceMarker::Intent { prose: Some(p), is_doc: false, line: 1 } if p == "Must reject negative amounts")
        );
    }

    #[test]
    fn scan_alias_chinese() {
        let m = scan_markers("// \x40立意:忽略\n// \x40立意:模块\n");
        assert_eq!(m.len(), 2);
        assert!(matches!(&m[0], SourceMarker::Ignore { .. }));
        assert!(matches!(&m[1], SourceMarker::Module { .. }));
    }

    #[test]
    fn scan_fullwidth_normalization() {
        let m = scan_markers("// \u{FF20}立意\u{FF1A}忽略\n");
        assert_eq!(m.len(), 1);
        assert!(matches!(
            &m[0],
            SourceMarker::Ignore {
                reason: None,
                line: 1
            }
        ));
    }

    // -----------------------------------------------------------------------
    // NL-quoting quine suppression tests
    // -----------------------------------------------------------------------

    #[test]
    fn fenced_block_suppresses_markers() {
        let input = "before\n```\n// \x40liyi:module\n```\nafter\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker inside fenced block should be suppressed"
        );
    }

    #[test]
    fn fenced_block_tilde_suppresses_markers() {
        let input = "before\n~~~\n// \x40liyi:trivial\n~~~\nafter\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker inside ~~~ fenced block should be suppressed"
        );
    }

    #[test]
    fn marker_after_fenced_block_still_found() {
        let input = "```\n// \x40liyi:module\n```\n// \x40liyi:trivial\n";
        let m = scan_markers(input);
        assert_eq!(m.len(), 1);
        assert!(matches!(&m[0], SourceMarker::Trivial { line: 4 }));
    }

    #[test]
    fn inline_backtick_suppresses_marker() {
        let input = "use `\x40liyi:module` in your code\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker inside inline backticks should be suppressed"
        );
    }

    #[test]
    fn inline_backtick_with_surrounding_text() {
        // Pattern from design doc: `<!-- @liyi:module -->`
        let input = "The `<!-- \x40liyi:module -->` comment marks the block\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker inside backtick span with surrounding text should be suppressed"
        );
    }

    #[test]
    fn preceding_double_quote_suppresses() {
        let input = "the string \"\x40liyi:intent\" is used\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker preceded by double quote should be suppressed"
        );
    }

    #[test]
    fn preceding_single_quote_suppresses() {
        let input = "the string '\x40liyi:module' is used\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker preceded by single quote should be suppressed"
        );
    }

    #[test]
    fn preceding_curly_quote_suppresses() {
        let input = "mention \u{201C}\x40liyi:intent\u{201D} in docs\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker preceded by curly quote should be suppressed"
        );
    }

    #[test]
    fn preceding_cjk_bracket_suppresses() {
        let input = "use \u{300C}\x40liyi:requirement\u{300D}\n";
        let m = scan_markers(input);
        assert!(
            m.is_empty(),
            "marker preceded by CJK bracket should be suppressed"
        );
    }

    #[test]
    fn html_comment_marker_not_suppressed() {
        // Real markers inside HTML comments should be detected
        let input = "<!-- \x40liyi:module -->\n";
        let m = scan_markers(input);
        assert_eq!(m.len(), 1);
        assert!(matches!(&m[0], SourceMarker::Module { line: 1 }));
    }

    #[test]
    fn source_comment_marker_not_suppressed() {
        // Normal source comment markers should be detected
        let input = "// \x40liyi:requirement(auth-check)\n";
        let m = scan_markers(input);
        assert_eq!(m.len(), 1);
        assert!(
            matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "auth-check")
        );
    }

    #[test]
    fn mixed_real_and_documentary_markers() {
        // A realistic Markdown file: real marker + fenced example + inline mention
        let input = "\
<!-- \x40liyi:requirement(exit-codes) -->\n\
Exit codes: 0 = clean, 1 = failures.\n\
<!-- /requirement -->\n\
\n\
### Example\n\
\n\
```\n\
// \x40liyi:intent Add two amounts\n\
```\n\
\n\
Use `\x40liyi:intent` to annotate functions.\n\
";
        let m = scan_markers(input);
        assert_eq!(
            m.len(),
            1,
            "only the real requirement marker should be found"
        );
        assert!(
            matches!(&m[0], SourceMarker::Requirement { name, line: 1 } if name == "exit-codes")
        );
    }
}
