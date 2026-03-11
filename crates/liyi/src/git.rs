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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_show_returns_none_for_nonexistent_path() {
        // Even in a real repo, a bogus path should return None.
        let result = git_show(Path::new("."), "nonexistent/file.txt", "HEAD");
        assert!(result.is_none());
    }
}
