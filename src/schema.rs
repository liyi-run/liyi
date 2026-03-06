use crate::sidecar::SidecarFile;

const CURRENT_VERSION: &str = "0.1";

/// Accept version "0.1" only; return an error for anything else.
pub fn validate_version(version: &str) -> Result<(), String> {
    if version == CURRENT_VERSION {
        Ok(())
    } else {
        Err(format!(
            "unsupported schema version \"{version}\", expected \"{CURRENT_VERSION}\""
        ))
    }
}

/// Migrate a sidecar file to the current schema version.
///
/// In v0.1 this is a no-op — the scaffold exists so `--migrate` works from
/// day one and future versions can add real migration logic here.
pub fn migrate(sidecar: &mut SidecarFile) -> Result<(), String> {
    validate_version(&sidecar.version)
}
