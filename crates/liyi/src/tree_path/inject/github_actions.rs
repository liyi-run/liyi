//! GitHub Actions injection profile.
//!
//! Activates for `.github/workflows/**/*.yml|yaml` and
//! `.github/actions/**/*.yml|yaml`. Injects `run:` values under `jobs:` as
//! Bash.

use super::{InjectionProfile, InjectionRule};
use crate::tree_path::Language;

pub(crate) static PROFILE: InjectionProfile = InjectionProfile {
    host: Language::Yaml,
    path_patterns: &[
        ".github/workflows/**/*.yml",
        ".github/workflows/**/*.yaml",
        ".github/actions/**/*.yml",
        ".github/actions/**/*.yaml",
    ],
    rules: &[InjectionRule {
        key_name: "run",
        language: Language::Bash,
        ancestor_keys: &["jobs"],
    }],
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn profile_matches_workflow_yml() {
        assert!(PROFILE.matches_path(Path::new(".github/workflows/ci.yml")));
    }

    #[test]
    fn profile_matches_workflow_yaml() {
        assert!(PROFILE.matches_path(Path::new(".github/workflows/build.yaml")));
    }

    #[test]
    fn profile_matches_nested_workflow() {
        assert!(PROFILE.matches_path(Path::new(".github/workflows/nested/deploy.yml")));
    }

    #[test]
    fn profile_matches_action() {
        assert!(PROFILE.matches_path(Path::new(".github/actions/setup/action.yml")));
    }

    #[test]
    fn profile_rejects_plain_yaml() {
        assert!(!PROFILE.matches_path(Path::new("config.yaml")));
    }

    #[test]
    fn profile_rejects_k8s() {
        assert!(!PROFILE.matches_path(Path::new("k8s/deploy.yaml")));
    }

    #[test]
    fn rule_fires_with_jobs_ancestor() {
        let rule = PROFILE.find_rule("run", &["jobs"]);
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().language, Language::Bash);
    }

    #[test]
    fn rule_fires_with_deep_ancestor() {
        let rule = PROFILE.find_rule("run", &["name", "jobs", "build", "steps"]);
        assert!(rule.is_some());
    }

    #[test]
    fn rule_does_not_fire_without_jobs() {
        assert!(PROFILE.find_rule("run", &[]).is_none());
        assert!(PROFILE.find_rule("run", &["name", "on"]).is_none());
    }

    #[test]
    fn rule_does_not_fire_for_other_keys() {
        assert!(PROFILE.find_rule("uses", &["jobs"]).is_none());
        assert!(PROFILE.find_rule("name", &["jobs"]).is_none());
    }
}
