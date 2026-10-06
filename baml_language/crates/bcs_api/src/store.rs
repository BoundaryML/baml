//! Endpoint-scoped login files shared by every native BAML host.
//!
//! These files are private to the OS user, not encrypted. A process running as
//! that user can read them. Access grants belong to the in-memory auth cache.
use std::{
    fmt, fs,
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
    auth::{Endpoint, Secret, StoredSession},
    error::{Error, Result},
};

const MAX_LOGIN_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStoreLocation {
    pub(crate) path: PathBuf,
}

impl fmt::Display for CredentialStoreLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.display().fmt(f)
    }
}

pub struct Store {
    location: CredentialStoreLocation,
}

impl Store {
    /// Construction and reads never create files. Only login/refresh writes.
    pub fn new(endpoint: &Endpoint) -> Result<Self> {
        let home = baml_env::baml_home_from(baml_env::os_var("BAML_HOME"), dirs::home_dir());
        Ok(Self::in_home(endpoint, &home))
    }

    fn in_home(endpoint: &Endpoint, home: &Path) -> Self {
        let name = hex::encode(Sha256::digest(endpoint.as_str().as_bytes()));
        Self {
            location: CredentialStoreLocation {
                path: home.join("auth").join(format!("{name}.json")),
            },
        }
    }

    fn storage_error(&self, operation: &'static str, source: io::Error) -> Error {
        Error::Storage {
            operation,
            location: self.location.clone(),
            source,
        }
    }

    pub fn read(&self) -> Result<Option<StoredSession>> {
        match fs::symlink_metadata(&self.location.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.storage_error("read", error)),
            Ok(metadata) if !metadata.is_file() => {
                return Err(self.storage_error(
                    "read",
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Expected a regular credential file",
                    ),
                ));
            }
            Ok(_) => {}
        }
        let file = match fs::File::open(&self.location.path) {
            Ok(file) => file,
            // Another host may have logged out since the metadata check.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.storage_error("read", error)),
        };
        let mut bytes = Vec::new();
        file.take(MAX_LOGIN_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| self.storage_error("read", error))?;
        let invalid = || Error::InvalidStoredLogin {
            location: self.location.clone(),
        };
        if bytes.len() as u64 > MAX_LOGIN_BYTES {
            return Err(invalid());
        }
        // Parser errors can echo credentials. Never retain the parser message.
        let mut session: StoredSession = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if !session
            .refresh_token
            .expose()
            .starts_with(crate::credentials::SESSION_PREFIX)
        {
            session.refresh_token = Secret::new(format!(
                "{}{}",
                crate::credentials::SESSION_PREFIX,
                session.refresh_token.expose()
            ));
        }
        Ok(Some(session))
    }

    pub fn write(&self, session: &StoredSession) -> Result<()> {
        let bytes = serde_json::to_vec(session)?;
        if bytes.len() as u64 > MAX_LOGIN_BYTES {
            return Err(self.storage_error(
                "save",
                io::Error::new(io::ErrorKind::InvalidData, "Saved login exceeds size limit"),
            ));
        }
        self.write_atomic(&bytes)
            .map_err(|error| self.storage_error("save", error))
    }

    fn write_atomic(&self, bytes: &[u8]) -> io::Result<()> {
        let directory = self.location.path.parent().expect("auth file has a parent");
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        if !fs::symlink_metadata(directory)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected a regular credential directory",
            ));
        }
        // Secure the directory before any file can contain a credential.
        make_private(directory, true)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        make_private(temporary.path(), false)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.location.path)
            .map_err(|error| error.error)?;
        #[cfg(unix)]
        fs::File::open(directory)?.sync_all()?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.location.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.storage_error("remove", error)),
        }
    }
}

#[cfg(unix)]
fn make_private(path: &Path, directory: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows requires native ACL APIs to restrict files to their owner"
)]
fn make_private(path: &Path, directory: bool) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt as _, ptr};

    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
        },
    };

    // Protected DACL: full access only for the object's owner. Directory ACEs
    // inherit to newly created children, including the temporary login file.
    let sddl = if directory {
        "D:P(A;OICI;FA;;;OW)"
    } else {
        "D:P(A;;FA;;;OW)"
    };
    let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Path contains a null character",
        ));
    }
    name.push(0);
    let mut descriptor = ptr::null_mut();
    // SAFETY: inputs are NUL-terminated; descriptor is freed exactly once
    // after SetFileSecurityW finishes borrowing it.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let result = SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        );
        let error = if result == 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        LocalFree(descriptor);
        error.map_or(Ok(()), Err)
    }
}

#[cfg(not(any(unix, windows)))]
fn make_private(_path: &Path, _directory: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Private credential files are unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Caller;

    fn session(token: &str) -> StoredSession {
        StoredSession {
            refresh_token: Secret::new(token.to_owned()),
            caller: Caller {
                user_id: "user_1".into(),
                email: "user@example.com".into(),
            },
        }
    }

    #[test]
    fn absent_read_is_side_effect_free_and_logout_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        assert!(store.read().unwrap().is_none());
        store.clear().unwrap();
        assert!(!home.path().join("auth").exists());
    }

    #[test]
    fn hosts_share_endpoint_files_and_other_endpoints_or_homes_are_isolated() {
        let home = tempfile::tempdir().unwrap();
        let first = Store::in_home(
            &Endpoint::parse("https://EXAMPLE.com:443/").unwrap(),
            home.path(),
        );
        let second = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        first.write(&session("bdry_session_first")).unwrap();
        assert_eq!(
            second.read().unwrap().unwrap().refresh_token.expose(),
            "bdry_session_first"
        );
        let other = Store::in_home(
            &Endpoint::parse("https://other.example.com").unwrap(),
            home.path(),
        );
        assert!(other.read().unwrap().is_none());
        let other_home = tempfile::tempdir().unwrap();
        let other = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            other_home.path(),
        );
        assert!(other.read().unwrap().is_none());
        second.write(&session("bdry_session_second")).unwrap();
        assert_eq!(
            first.read().unwrap().unwrap().refresh_token.expose(),
            "bdry_session_second"
        );
        first.clear().unwrap();
        assert!(second.read().unwrap().is_none());
        first.clear().unwrap();
    }

    #[test]
    fn corrupt_or_oversized_login_is_an_error_without_credential_contents() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        store.write(&session("bdry_session_valid")).unwrap();
        for contents in [
            b"{bdry_session_do_not_print".to_vec(),
            vec![b'x'; usize::try_from(MAX_LOGIN_BYTES).unwrap() + 1],
        ] {
            fs::write(&store.location.path, contents).unwrap();
            let error = store.read().unwrap_err();
            assert!(matches!(error, Error::InvalidStoredLogin { .. }));
            assert_eq!(
                error.to_string(),
                format!(
                    "Invalid saved Boundary login in the credential file at {}",
                    store.location
                )
            );
        }
    }

    #[test]
    fn inaccessible_store_is_not_an_absent_login() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("auth"), "not a directory").unwrap();
        let store = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        assert!(matches!(
            store.read(),
            Err(Error::Storage {
                operation: "read",
                ..
            })
        ));
        assert!(matches!(
            store.write(&session("bdry_session_test")),
            Err(Error::Storage {
                operation: "save",
                ..
            })
        ));
    }

    #[test]
    fn simultaneous_hosts_never_observe_a_partially_written_login() {
        let home = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::parse("https://example.com").unwrap();
        let store = Store::in_home(&endpoint, home.path());
        store.write(&session("bdry_session_first")).unwrap();
        std::thread::scope(|scope| {
            for token in ["bdry_session_first", "bdry_session_second"] {
                let store = &store;
                scope.spawn(move || {
                    for _ in 0..32 {
                        store.write(&session(token)).unwrap();
                    }
                });
            }
            for _ in 0..128 {
                let saved = store.read().unwrap().unwrap();
                assert!(matches!(
                    saved.refresh_token.expose(),
                    "bdry_session_first" | "bdry_session_second"
                ));
                assert_eq!(saved.caller.email, "user@example.com");
            }
        });
        assert_eq!(
            fs::read_dir(home.path().join("auth")).unwrap().count(),
            1,
            "atomic writes must clean up temporary files"
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_files_and_directories_are_owner_only_after_each_replacement() {
        use std::os::unix::fs::PermissionsExt as _;
        let home = tempfile::tempdir().unwrap();
        let store = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        let directory = home.path().join("auth");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        for token in ["bdry_session_first", "bdry_session_second"] {
            store.write(&session(token)).unwrap();
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&store.location.path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_files_and_directories() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let store = Store::in_home(
            &Endpoint::parse("https://example.com").unwrap(),
            home.path(),
        );
        symlink(other.path(), home.path().join("auth")).unwrap();
        assert!(matches!(
            store.write(&session("bdry_session_test")),
            Err(Error::Storage { .. })
        ));
        fs::remove_file(home.path().join("auth")).unwrap();
        fs::create_dir(home.path().join("auth")).unwrap();
        let target = other.path().join("login.json");
        fs::write(&target, "do not modify").unwrap();
        symlink(&target, &store.location.path).unwrap();
        assert!(matches!(store.read(), Err(Error::Storage { .. })));
        store.clear().unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "do not modify");
    }
}
