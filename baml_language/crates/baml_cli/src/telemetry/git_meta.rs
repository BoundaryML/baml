//! Best-effort Git metadata for the repository containing the invocation.
//!
//! The raw origin URL is never retained or sent because it can contain
//! credentials. Only its parsed host, top-level organization, and repository
//! path are exposed to telemetry.

use std::{process::Command, sync::OnceLock};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct GitMeta {
    pub origin_host: Option<String>,
    pub origin_org: Option<String>,
    pub origin_repo: Option<String>,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub committer_name: Option<String>,
    pub committer_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitOrigin {
    host: String,
    org: String,
    repo: String,
}

pub(super) fn get() -> &'static GitMeta {
    static META: OnceLock<GitMeta> = OnceLock::new();
    META.get_or_init(compute)
}

fn compute() -> GitMeta {
    // One local Git process yields config plus both effective identities.
    // This performs no network access and keeps invocation overhead bounded.
    let vars = git_output(&["var", "-l"]).unwrap_or_default();
    let origin = git_var(&vars, "remote.origin.url").and_then(parse_origin);
    let author = git_var(&vars, "GIT_AUTHOR_IDENT").and_then(parse_ident);
    let committer = git_var(&vars, "GIT_COMMITTER_IDENT").and_then(parse_ident);

    GitMeta {
        origin_host: origin.as_ref().map(|origin| origin.host.clone()),
        origin_org: origin.as_ref().map(|origin| origin.org.clone()),
        origin_repo: origin.map(|origin| origin.repo),
        author_name: author.as_ref().map(|(name, _)| name.clone()),
        author_email: author.map(|(_, email)| email),
        committer_name: committer.as_ref().map(|(name, _)| name.clone()),
        committer_email: committer.map(|(_, email)| email),
    }
}

fn git_var<'a>(vars: &'a str, name: &str) -> Option<&'a str> {
    vars.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_origin(value: &str) -> Option<GitOrigin> {
    let value = value.trim();
    let (authority, path) = if let Some((_, rest)) = value.split_once("://") {
        rest.split_once('/')?
    } else {
        // Git's SCP-like syntax: `[user@]host:path`.
        value.split_once(':')?
    };

    let host_with_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host_with_port
        .split_once(':')
        .map_or(host_with_port, |(host, _)| host)
        .trim();
    let repo = path
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| path.trim_matches('/'))
        .trim_matches('/');
    let org = repo.split('/').next()?.trim();

    if host.is_empty() || org.is_empty() || !repo.contains('/') {
        return None;
    }

    Some(GitOrigin {
        host: host.to_ascii_lowercase(),
        org: org.to_string(),
        repo: repo.to_string(),
    })
}

fn parse_ident(value: &str) -> Option<(String, String)> {
    // `git var GIT_*_IDENT` returns `Name <email> timestamp timezone`.
    let email_end = value.rfind('>')?;
    let email_start = value[..email_end].rfind('<')?;
    let name = value[..email_start].trim();
    let email = value[email_start + 1..email_end].trim();
    if name.is_empty() || email.is_empty() {
        return None;
    }
    Some((name.to_string(), email.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_origin_url_variants() {
        for value in [
            "https://github.com/BoundaryML/baml",
            "https://github.com/BoundaryML/baml.git",
            "git://github.com/BoundaryML/baml.git",
            "git@github.com:BoundaryML/baml.git",
            "ssh://git@github.com/BoundaryML/baml.git",
            "ssh://git@github.com:22/BoundaryML/baml.git",
        ] {
            assert_eq!(
                parse_origin(value),
                Some(GitOrigin {
                    host: "github.com".to_string(),
                    org: "BoundaryML".to_string(),
                    repo: "BoundaryML/baml".to_string(),
                }),
                "origin: {value}",
            );
        }
    }

    #[test]
    fn strips_credentials_without_exposing_them() {
        assert_eq!(
            parse_origin("https://user:secret@github.com/BoundaryML/baml.git"),
            Some(GitOrigin {
                host: "github.com".to_string(),
                org: "BoundaryML".to_string(),
                repo: "BoundaryML/baml".to_string(),
            }),
        );
    }

    #[test]
    fn preserves_nested_repository_paths() {
        assert_eq!(
            parse_origin("git@gitlab.example.com:top-level/team/project.git"),
            Some(GitOrigin {
                host: "gitlab.example.com".to_string(),
                org: "top-level".to_string(),
                repo: "top-level/team/project".to_string(),
            }),
        );
    }

    #[test]
    fn rejects_local_and_incomplete_origins() {
        assert_eq!(parse_origin("../local-repo"), None);
        assert_eq!(parse_origin("https://github.com/BoundaryML"), None);
    }

    #[test]
    fn parses_git_identity_without_timestamp() {
        assert_eq!(
            parse_ident("BAML Agent <agent@example.com> 1788382221 -0700"),
            Some(("BAML Agent".to_string(), "agent@example.com".to_string(),)),
        );
    }

    #[test]
    fn extracts_values_from_git_var_listing() {
        let vars = "remote.origin.url=https://github.com/BoundaryML/baml.git\nGIT_COMMITTER_IDENT=BAML Agent <agent@example.com> 1788382221 -0700\n";
        assert_eq!(
            git_var(vars, "remote.origin.url"),
            Some("https://github.com/BoundaryML/baml.git"),
        );
        assert_eq!(
            git_var(vars, "GIT_COMMITTER_IDENT"),
            Some("BAML Agent <agent@example.com> 1788382221 -0700"),
        );
    }
}
