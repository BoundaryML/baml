//! The record of one build (BEP-088): which commit the package was built
//! from, the hash of its formatted source, and the package it produced.
//!
//! Reading git and formatting every source file are not free, so a record is
//! described by a provider the engine calls only when a recording that keeps
//! it starts.

use std::path::Path;

use baml_db::{ProjectDatabase, SourceRoot};
use btel_types::{BuildOrigin, BuildRecord, GitHash, GitInfo};
use gix::bstr::{BString, ByteSlice};

/// A provider of the record of this build of `package`, whose compiled form
/// is `program`'s root. `None` when the program's root records no unit hash,
/// which leaves a build of it nothing to name.
pub(crate) fn provider(
    db: &ProjectDatabase,
    package: SourceRoot,
    program: &bex_vm_types::Program,
    origin: BuildOrigin,
) -> Option<bex_engine::BuildProvider> {
    let unit = program
        .packages
        .get(program.root as usize)?
        .metadata
        .as_ref()?
        .compilation_unit_hash;
    let timestamp_unix_ns = unix_now_ns();
    let db = db.clone();
    Some(Box::new(move || {
        let source = match baml_fmt::package_source::package_source(&db, package) {
            Ok(source) => source,
            Err(error) => {
                crate::reporter::print_warning(format_args!(
                    "this build is not recorded in telemetry: {error}"
                ));
                return None;
            }
        };
        Some(BuildRecord {
            build_id: *uuid::Uuid::new_v4().as_bytes(),
            timestamp_unix_ns,
            origin,
            git: git_info(package.path(&db)),
            source_hash: bex_vm_types::identity::package_source_hash(
                source
                    .files
                    .iter()
                    .map(|(path, text)| (path.as_str(), text.as_bytes())),
            )
            .0,
            compilation_unit_hash: unit.0,
        })
    }))
}

fn unix_now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_nanos()).ok())
        .unwrap_or(0)
}

/// Where the package at `package_root` sits in git, or `None` when it is not
/// in a repository with a work tree and a commit at `HEAD`.
pub(crate) fn git_info(package_root: &Path) -> Option<GitInfo> {
    let repo = gix::discover(package_root).ok()?;
    let workdir = repo.workdir()?.canonicalize().ok()?;
    let head = repo.head_id().ok()?;
    let hash = match head.as_bytes() {
        bytes if bytes.len() == 20 => GitHash::Sha1(bytes.try_into().ok()?),
        bytes => GitHash::Sha256(bytes.try_into().ok()?),
    };
    let package_path = package_root
        .canonicalize()
        .ok()?
        .strip_prefix(&workdir)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string())
        .or_else(ci_branch);
    let remote = repo
        .find_remote("origin")
        .ok()
        .and_then(|origin| https_form(origin.url(gix::remote::Direction::Fetch)?));
    Some(GitInfo {
        remote,
        hash,
        // A status that cannot be read proves nothing clean.
        dirty: package_is_dirty(&repo, &package_path).unwrap_or(true),
        branch,
        package_path,
    })
}

/// Whether anything under `package_path` differs from `HEAD`: staged,
/// modified, deleted, or untracked and not ignored. Scoped to the package,
/// so unrelated edits elsewhere in the repository do not mark its build.
fn package_is_dirty(repo: &gix::Repository, package_path: &str) -> Option<bool> {
    let patterns: Vec<BString> = if package_path.is_empty() {
        Vec::new()
    } else {
        vec![format!(":(top,literal){package_path}").into()]
    };
    let mut changes = repo
        .status(gix::progress::Discard)
        .ok()?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(patterns)
        .ok()?;
    match changes.next() {
        None => Some(false),
        Some(Ok(_)) => Some(true),
        Some(Err(_)) => None,
    }
}

/// The branch a CI checked out, for the detached `HEAD` it usually leaves.
fn ci_branch() -> Option<String> {
    [
        "GITHUB_REF_NAME",
        "CI_COMMIT_REF_NAME",
        "BUILDKITE_BRANCH",
        "CIRCLE_BRANCH",
    ]
    .into_iter()
    .find_map(|name| baml_env::os_var(name)?.into_string().ok())
    .filter(|branch| !branch.is_empty())
}

/// A remote's URL in `https` form with no credentials, so one repository is
/// one remote however it was cloned. `None` for a remote with no host (a
/// local path), which has no such form.
fn https_form(url: &gix::Url) -> Option<String> {
    let host = url.host.as_deref()?;
    // A port names the transport's endpoint: it carries over only from a URL
    // that already spoke HTTP.
    let port = match url.scheme {
        gix::url::Scheme::Http | gix::url::Scheme::Https => url.port,
        _ => None,
    };
    let path = url.path.to_str().ok()?.trim_start_matches('/');
    Some(match port {
        Some(port) => format!("https://{host}:{port}/{path}"),
        None => format!("https://{host}/{path}"),
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use super::*;

    fn https(url: &str) -> Option<String> {
        https_form(&gix::url::parse(url.as_bytes().as_bstr()).unwrap())
    }

    #[test]
    fn remotes_take_one_https_form_without_credentials() {
        let expected = Some("https://github.com/acme/app.git".to_owned());
        for url in [
            "git@github.com:acme/app.git",
            "ssh://git@github.com/acme/app.git",
            "ssh://git@github.com:22/acme/app.git",
            "https://github.com/acme/app.git",
            "https://user:secret@github.com/acme/app.git",
            "http://github.com/acme/app.git",
            "git://github.com/acme/app.git",
        ] {
            assert_eq!(https(url), expected, "{url}");
        }
        assert_eq!(
            https("https://user:secret@git.example.com:8443/acme/app.git").as_deref(),
            Some("https://git.example.com:8443/acme/app.git")
        );
        assert_eq!(https("/srv/git/app.git"), None);
        assert_eq!(https("file:///srv/git/app.git"), None);
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    }

    fn head(repo: &Path) -> Vec<u8> {
        let output = Command::new("git")
            .current_dir(repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git runs");
        let hex = String::from_utf8(output.stdout).unwrap();
        (0..hex.trim().len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    }

    /// A repository with a committed package at `pkg/` and a committed file
    /// outside it.
    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        git(repo, &["init", "--quiet", "--initial-branch", "trunk"]);
        fs::create_dir_all(repo.join("pkg/baml_src")).unwrap();
        fs::write(
            repo.join("pkg/baml_src/main.baml"),
            "function f() -> int { 1 }\n",
        )
        .unwrap();
        fs::write(repo.join("README.md"), "readme\n").unwrap();
        fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "--quiet", "-m", "initial"]);
        dir
    }

    #[test]
    fn a_clean_package_reports_its_commit_branch_and_path() {
        let dir = repository();
        let repo = dir.path();
        git(
            repo,
            &["remote", "add", "origin", "git@github.com:acme/app.git"],
        );
        let info = git_info(&repo.join("pkg")).expect("the package is in git");
        assert_eq!(info.hash.as_bytes(), head(repo));
        assert_eq!(info.branch.as_deref(), Some("trunk"));
        assert_eq!(info.package_path, "pkg");
        assert_eq!(
            info.remote.as_deref(),
            Some("https://github.com/acme/app.git")
        );
        assert!(!info.dirty);

        let at_root = git_info(repo).expect("the root is in git");
        assert_eq!(at_root.package_path, "");
        assert!(!at_root.dirty);
    }

    #[test]
    fn only_changes_inside_the_package_make_it_dirty() {
        let dir = repository();
        let repo = dir.path();
        let dirty = || git_info(&repo.join("pkg")).unwrap().dirty;

        fs::write(repo.join("README.md"), "edited\n").unwrap();
        fs::write(repo.join("untracked.txt"), "new\n").unwrap();
        assert!(!dirty(), "changes outside the package do not count");
        assert!(git_info(repo).unwrap().dirty, "they do for the root");

        fs::create_dir_all(repo.join("pkg/target")).unwrap();
        fs::write(repo.join("pkg/target/out"), "ignored\n").unwrap();
        assert!(!dirty(), "an ignored file is not a change");

        fs::write(repo.join("pkg/baml_src/new.baml"), "").unwrap();
        assert!(dirty(), "an untracked source file is a change");
        fs::remove_file(repo.join("pkg/baml_src/new.baml")).unwrap();
        assert!(!dirty());

        fs::write(
            repo.join("pkg/baml_src/main.baml"),
            "function f() -> int { 2 }\n",
        )
        .unwrap();
        assert!(dirty(), "a modified file is a change");
        git(repo, &["add", "pkg"]);
        assert!(dirty(), "a staged change is still ahead of the commit");
        git(repo, &["commit", "--quiet", "-m", "edit"]);
        assert!(!dirty());

        fs::remove_file(repo.join("pkg/baml_src/main.baml")).unwrap();
        assert!(dirty(), "a deleted file is a change");
    }

    #[test]
    fn a_detached_head_has_no_branch_and_a_bare_directory_no_git() {
        let dir = repository();
        let repo = dir.path();
        git(repo, &["checkout", "--quiet", "--detach"]);
        let info = git_info(&repo.join("pkg")).unwrap();
        assert_eq!(info.hash.as_bytes(), head(repo));
        // A CI's own variable may name the branch; nothing else does.
        assert_eq!(info.branch, ci_branch());
        assert_eq!(info.remote, None);

        let plain = tempfile::tempdir().unwrap();
        // The temporary directory may itself sit under a repository; only a
        // directory outside every repository proves the `None`.
        if gix::discover(plain.path()).is_err() {
            assert_eq!(git_info(plain.path()), None);
        }
    }
}
