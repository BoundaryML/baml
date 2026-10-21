use std::{env, process::Command};

/// Runs `git` against the checkout being built.
///
/// `safe.directory=*` lets Git read a checkout owned by another uid, as in CI
/// job containers, `cross`/maturin build containers, and dev containers. It
/// grants nothing new: this build script already runs code from the same
/// checkout.
fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(["-c", "safe.directory=*"])
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn track_git_head() {
    let Some(head) = git_output(&["rev-parse", "--git-path", "HEAD"]) else {
        return;
    };
    println!("cargo:rerun-if-changed={head}");

    if let Some(reference) = git_output(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
}

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
    track_git_head();

    // An explicit `BAML_GIT_SHA` (release CI, source archives) wins over the
    // checkout's HEAD. An empty value counts as unset.
    let commit = match env::var("BAML_GIT_SHA") {
        Ok(value) if !value.trim().is_empty() => {
            let value = value.trim();
            assert!(
                is_commit_id(value),
                "BAML_GIT_SHA must be a full lowercase Git commit id, got {value:?}"
            );
            Some(value.to_owned())
        }
        Ok(_) | Err(env::VarError::NotPresent) => {
            git_output(&["rev-parse", "HEAD"]).filter(|head| is_commit_id(head))
        }
        Err(env::VarError::NotUnicode(value)) => {
            panic!("BAML_GIT_SHA must be a full lowercase Git commit id, got {value:?}")
        }
    };

    // Empty when no commit is known. `lib.rs` decides whether the stamped
    // channel may build without one.
    println!(
        "cargo:rustc-env=BAML_ARTIFACT_BUILD_COMMIT={}",
        commit.unwrap_or_default()
    );
}
