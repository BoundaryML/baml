//! Shared local trace-store resolution for native writers and readers.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    tracing: Tracing,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tracing {
    store: Option<PathBuf>,
}

/// Resolve the state directory containing `btel/recordings` and `btel/cas`.
/// A project's `[tracing].store` wins over nonempty `BAML_HOME`, then the
/// project's `.baml`, then the user home. Relative manifest paths belong to
/// the manifest's directory; relative environment paths belong to the cwd.
/// This only reads configuration; it never creates directories or falls back
/// to a different store when the selected destination is unwritable.
pub fn resolve(
    project: Option<&Path>,
    baml_home: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> io::Result<PathBuf> {
    if let Some(project) = project {
        let path = project.join("baml.toml");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("{}: {error}", path.display()),
                ));
            }
        };
        if let Some(text) = text {
            let manifest: Manifest = toml::from_str(&text).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: invalid [tracing] settings: {error}", path.display()),
                )
            })?;
            if let Some(store) = manifest.tracing.store {
                if store.as_os_str().is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}: tracing.store must not be empty", path.display()),
                    ));
                }
                return Ok(project.join(store));
            }
        }
    }
    if let Some(home) = baml_home.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    Ok(project.map_or_else(
        || baml_env::baml_home_from(None, home_dir),
        |project| project.join(".baml"),
    ))
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

    #[test]
    fn manifest_home_project_and_user_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("app");
        let nested = project.join("src/deep");
        std::fs::create_dir_all(&nested).unwrap();
        let home = tmp.path().join("user");
        let state = tmp.path().join("state");
        let resolve =
            |baml_home| resolve_for_cwd(Some(&nested), baml_home, Some(home.clone())).unwrap();
        assert_eq!(resolve(None), home.join(".baml"));
        std::fs::write(project.join("baml.toml"), "[package]\nname = 'app'\n").unwrap();
        assert_eq!(resolve(None), project.join(".baml"));
        assert_eq!(resolve(Some("".into())), project.join(".baml"));
        assert_eq!(resolve(Some(state.clone().into())), state);
        std::fs::write(project.join("baml.toml"), "[tracing]\nstore = 'traces'\n").unwrap();
        assert_eq!(resolve(Some(state.clone().into())), project.join("traces"));
        let config = format!("[tracing]\nstore = {:?}\n", state.to_str().unwrap());
        std::fs::write(project.join("baml.toml"), config).unwrap();
        assert_eq!(resolve(None), state);
        // Packed programs ignore the manifest even when launched in a project.
        assert_eq!(
            super::resolve_for_cwd(None, None, Some(home.clone())).unwrap(),
            home.join(".baml")
        );
    }

    #[test]
    fn invalid_settings_are_actionable_and_do_not_silently_select_another_store() {
        let tmp = tempfile::tempdir().unwrap();
        for text in [
            "[tracing]\nstore = ''",
            "[tracing]\nstore = 42",
            "[tracing]\nstroe = 'traces'",
            "tracing = 'traces'",
            "not toml",
        ] {
            std::fs::write(tmp.path().join("baml.toml"), text).unwrap();
            let error = resolve(Some(tmp.path()), Some("elsewhere".into()), None).unwrap_err();
            assert!(error.to_string().contains("baml.toml"), "{error}");
            assert!(error.to_string().contains("tracing"), "{error}");
        }
        assert!(!tmp.path().join(".baml").exists());
    }
}
