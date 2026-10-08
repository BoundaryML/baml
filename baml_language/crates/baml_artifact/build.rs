use std::env;

/// Whether `value` is a Git object name: 40 (SHA-1) or 64 (SHA-256) lowercase
/// hex digits. Lowercase only, so a commit has exactly one spelling and
/// fingerprints compare byte for byte.
fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn main() {
    println!("cargo:rerun-if-env-changed=BAML_GIT_SHA");

    // Only an explicit `BAML_GIT_SHA` stamps a commit, and only release builds
    // set one. The checkout's HEAD is deliberately not read: it would change
    // this crate, and so every crate downstream of it, on every commit, and
    // defeat the cargo and sccache caches of test and development builds. An
    // empty value counts as unset.
    let commit = match env::var("BAML_GIT_SHA") {
        Ok(value) if !value.trim().is_empty() => {
            let value = value.trim();
            assert!(
                is_commit_id(value),
                "BAML_GIT_SHA must be a full lowercase Git commit id, got {value:?}"
            );
            value.to_owned()
        }
        Ok(_) | Err(env::VarError::NotPresent) => String::new(),
        Err(env::VarError::NotUnicode(value)) => {
            panic!("BAML_GIT_SHA must be a full lowercase Git commit id, got {value:?}")
        }
    };

    // Empty when the build is unstamped. `lib.rs` picks the fingerprint such a
    // build carries.
    println!("cargo:rustc-env=BAML_ARTIFACT_BUILD_COMMIT={commit}");
}
