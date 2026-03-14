use std::path::Path;
use std::process::Command;

/// Retrieve the contents of a file at a given Git revision.
///
/// `repo_root` is the working directory for git (must contain `.git/`).
/// `repo_relative_path` is the path relative to the repo root (e.g.
/// `crates/liyi/src/approve.rs.liyi.jsonc`).
/// `revision` is a Git rev (e.g. `HEAD`, `HEAD~1`, a commit hash).
///
/// Returns `None` if git is unavailable, the file doesn't exist at that
/// revision, or the repo is not a git repository.
// @liyi:related no-git2-dependency
pub fn git_show(repo_root: &Path, repo_relative_path: &str, revision: &str) -> Option<String> {
    let object = format!("{revision}:{repo_relative_path}");
    let output = Command::new("git")
        .arg("show")
        .arg(&object)
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

/// Return commit hashes that touched `repo_relative_path`, most recent first.
///
/// Returns at most `limit` entries. Uses `git log --follow` to track renames.
/// Returns an empty vec if git is unavailable or the file has no history.
// @liyi:related no-git2-dependency
pub fn git_log_revisions(repo_root: &Path, repo_relative_path: &str, limit: usize) -> Vec<String> {
    let output = Command::new("git")
        .args([
            "log",
            "--follow",
            "--format=%H",
            &format!("-{limit}"),
            "--",
            repo_relative_path,
        ])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn walk_git_history<T>(
    repo_root: &Path,
    repo_relative_path: &str,
    limit: usize,
    mut f: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    for revision in git_log_revisions(repo_root, repo_relative_path, limit) {
        if let Some(value) = f(&revision) {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_show_returns_none_for_nonexistent_path() {
        // Even in a real repo, a bogus path should return None.
        let result = git_show(Path::new("."), "nonexistent/file.txt", "HEAD");
        assert!(result.is_none());
    }

    #[test]
    fn walk_git_history_returns_none_without_history() {
        let result = walk_git_history(Path::new("."), "nonexistent/file.txt", 20, |_| {
            Some("unexpected")
        });
        assert!(result.is_none());
    }
}
