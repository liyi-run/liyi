use crate::hashing::hash_span;
use crate::shift::{ShiftResult, detect_shift};
use crate::tree_path::{Language, resolve_tree_path, resolve_tree_path_sibling_scan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryMethod {
    TreePath,
    SiblingScan,
    ShiftHeuristic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemSpanRecovery {
    Unchanged {
        span: [usize; 2],
        updated_tree_path: Option<String>,
        method: RecoveryMethod,
    },
    Shifted {
        from: [usize; 2],
        to: [usize; 2],
        updated_tree_path: Option<String>,
        method: RecoveryMethod,
    },
    Failed {
        note: &'static str,
    },
}

impl ItemSpanRecovery {
    pub fn recovered_span(&self) -> Option<[usize; 2]> {
        match self {
            Self::Unchanged { span, .. } => Some(*span),
            Self::Shifted { to, .. } => Some(*to),
            Self::Failed { .. } => None,
        }
    }

    pub fn updated_tree_path(&self) -> Option<&str> {
        match self {
            Self::Unchanged {
                updated_tree_path, ..
            }
            | Self::Shifted {
                updated_tree_path, ..
            } => updated_tree_path.as_deref(),
            Self::Failed { .. } => None,
        }
    }

    pub fn method(&self) -> Option<RecoveryMethod> {
        match self {
            Self::Unchanged { method, .. } | Self::Shifted { method, .. } => Some(*method),
            Self::Failed { .. } => None,
        }
    }

    pub fn failure_note(&self) -> Option<&'static str> {
        match self {
            Self::Failed { note } => Some(*note),
            _ => None,
        }
    }
}

pub fn recover_item_span(
    source_content: &str,
    current_span: [usize; 2],
    tree_path: &str,
    lang: Option<Language>,
    expected_hash: Option<&str>,
) -> ItemSpanRecovery {
    let tree_path_note = if tree_path.is_empty() {
        "no tree_path set"
    } else if lang.is_none() {
        "no grammar for source language"
    } else {
        let language = lang.unwrap();
        if let Some(resolved_span) = resolve_tree_path(source_content, tree_path, language) {
            if let Some(old_hash) = expected_hash
                && hash_span(source_content, resolved_span)
                    .map(|(hash, _)| hash != old_hash)
                    .unwrap_or(false)
                && let Some(sibling) =
                    resolve_tree_path_sibling_scan(source_content, tree_path, language, old_hash)
            {
                return to_recovery(
                    current_span,
                    sibling.span,
                    Some(sibling.updated_tree_path),
                    RecoveryMethod::SiblingScan,
                );
            }

            return to_recovery(current_span, resolved_span, None, RecoveryMethod::TreePath);
        }

        "tree_path resolution failed"
    };

    if let (Some(language), Some(old_hash)) = (lang, expected_hash)
        && !tree_path.is_empty()
        && let Some(sibling) =
            resolve_tree_path_sibling_scan(source_content, tree_path, language, old_hash)
    {
        return to_recovery(
            current_span,
            sibling.span,
            Some(sibling.updated_tree_path),
            RecoveryMethod::SiblingScan,
        );
    }

    if let Some(old_hash) = expected_hash {
        return match detect_shift(source_content, current_span, old_hash) {
            ShiftResult::Shifted { new_span, .. } => {
                to_recovery(current_span, new_span, None, RecoveryMethod::ShiftHeuristic)
            }
            ShiftResult::Stale => ItemSpanRecovery::Failed {
                note: tree_path_note,
            },
        };
    }

    ItemSpanRecovery::Failed {
        note: tree_path_note,
    }
}

fn to_recovery(
    current_span: [usize; 2],
    recovered_span: [usize; 2],
    updated_tree_path: Option<String>,
    method: RecoveryMethod,
) -> ItemSpanRecovery {
    if recovered_span == current_span {
        ItemSpanRecovery::Unchanged {
            span: recovered_span,
            updated_tree_path,
            method,
        }
    } else {
        ItemSpanRecovery::Shifted {
            from: current_span,
            to: recovered_span,
            updated_tree_path,
            method,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_shift_heuristic_without_tree_path() {
        let old = "one\ntwo\nthree\n";
        let new = "zero\none\ntwo\nthree\n";
        let old_hash = hash_span(old, [2, 2]).unwrap().0;

        let recovery = recover_item_span(new, [2, 2], "", None, Some(&old_hash));

        assert_eq!(
            recovery,
            ItemSpanRecovery::Shifted {
                from: [2, 2],
                to: [3, 3],
                updated_tree_path: None,
                method: RecoveryMethod::ShiftHeuristic,
            }
        );
    }

    #[test]
    fn reports_failure_without_recovery_signal() {
        let recovery = recover_item_span("one\ntwo\n", [1, 1], "", None, None);

        assert_eq!(
            recovery,
            ItemSpanRecovery::Failed {
                note: "no tree_path set"
            }
        );
    }
}
