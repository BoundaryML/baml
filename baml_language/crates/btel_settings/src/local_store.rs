//! Shared local trace-store resolution for native writers and readers.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};

/// Resolve the state directory containing `btel/recordings` and `btel/cas`.
/// Project stores live at `<BAML_HOME>/<project-name>-<path-hash>`; without
/// `BAML_HOME`, the base is `~/.baml`. The canonical project path identifies
/// the store, so aliases share recordings and separate checkouts stay isolated.
/// Without a project, use the BAML home itself (including packed programs).
/// This never reads manifest settings, creates directories, or falls back to
/// a different store when the selected destination is unwritable.
pub fn resolve(
    project: Option<&Path>,
    baml_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> io::Result<PathBuf> {
    let home = baml_env::baml_home_from(baml_home, home_dir);
    let Some(project) = project else {
        return Ok(home);
    };
    let project = std::fs::canonicalize(project).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "{}: cannot resolve trace-store project: {error}",
                project.display()
            ),
        )
    })?;
    let name: String = project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .chars()
        .take(64)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let hash = format!(
        "{:x}",
        Sha256::digest(project.as_os_str().as_encoded_bytes())
    );
    Ok(home.join(format!("{name}-{}", &hash[..16])))
}

/// SDK hosts discover the project enclosing their working directory. Packed
/// applications pass no cwd so they do not adopt an unrelated project's store.
pub fn resolve_for_cwd(
    cwd: Option<&Path>,
    baml_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> io::Result<PathBuf> {
    let project = cwd.and_then(baml_project_root::find_baml_project_root);
    resolve(project.as_deref(), baml_home, home_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Project identity and nested discovery select one store without manifest configuration or writes.
    #[test]
    fn project_stores_live_under_baml_home_and_are_discovered_from_nested_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("app");
        let nested = project.join("baml_src/deep");
        std::fs::create_dir_all(&nested).unwrap();
        let home = tmp.path().join("user");
        let state = tmp.path().join("state");
        let default = resolve(Some(&project), None, Some(home.clone())).unwrap();
        assert_eq!(default.parent().unwrap(), home.join(".baml"));
        assert!(
            default
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("app-")
        );
        assert_eq!(
            resolve(Some(&project), Some("".into()), Some(home.clone())).unwrap(),
            default
        );
        let configured = resolve(Some(&project), Some(state.clone().into()), None).unwrap();
        assert_eq!(configured, state.join(default.file_name().unwrap()));
        assert_eq!(
            resolve_for_cwd(Some(&nested), Some(state.into()), None).unwrap(),
            configured
        );
        // Trace paths do not depend on manifest contents, including old settings.
        std::fs::write(
            project.join("baml.toml"),
            "[tracing]\nstore = 'elsewhere'\n",
        )
        .unwrap();
        assert_eq!(
            resolve(Some(&project), None, Some(home.clone())).unwrap(),
            default
        );
        assert_eq!(
            resolve_for_cwd(Some(&nested), None, Some(home)).unwrap(),
            default
        );
        assert!(!default.exists());
        assert!(!project.join(".baml").exists());
    }

    /// Canonical paths separate checkouts while equivalent path spellings share a store.
    #[test]
    fn separate_checkouts_with_the_same_name_have_separate_stores() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first/app");
        let second = tmp.path().join("second/app");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let home = Some(tmp.path().join("state").into());
        let store = resolve(Some(&first), home.clone(), None).unwrap();
        assert_ne!(store, resolve(Some(&second), home.clone(), None).unwrap());
        assert_eq!(
            store,
            resolve(Some(&first.join("../app")), home, None).unwrap()
        );
    }

    #[cfg(unix)]
    /// A symlink alias must not create another history for the same checkout.
    #[test]
    fn symlinked_project_uses_the_same_store() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("app");
        let alias = tmp.path().join("alias");
        std::fs::create_dir(&project).unwrap();
        std::os::unix::fs::symlink(&project, &alias).unwrap();
        let home = Some(tmp.path().join("state").into());
        assert_eq!(
            resolve(Some(&project), home.clone(), None).unwrap(),
            resolve(Some(&alias), home, None).unwrap()
        );
    }

    /// Packed and unassociated hosts retain the user store, including existing home fallbacks.
    #[test]
    fn no_project_uses_baml_home_itself() {
        let home = PathBuf::from("user");
        assert_eq!(
            resolve_for_cwd(None, None, Some(home.clone())).unwrap(),
            home.join(".baml")
        );
        assert_eq!(
            resolve(None, Some("state".into()), Some(home)).unwrap(),
            PathBuf::from("state")
        );
        assert_eq!(resolve(None, None, None).unwrap(), PathBuf::from(".baml"));
    }

    /// Project-resolution errors identify the requested path and never create a fallback store.
    #[test]
    fn missing_project_reports_the_path_instead_of_selecting_a_different_store() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing");
        let error =
            resolve(Some(&missing), Some(tmp.path().join("state").into()), None).unwrap_err();
        assert!(error.to_string().contains(missing.to_str().unwrap()));
        assert!(!tmp.path().join("state").exists());
    }
}
