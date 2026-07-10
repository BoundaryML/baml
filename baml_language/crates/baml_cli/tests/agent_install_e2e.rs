//! End-to-end test for `baml-cli agent install`'s default path: resolve the
//! skill repo head commit, download the tarball at that commit, install the
//! skills, and record provenance in `state.toml`.
//!
//! GitHub is stubbed with a local HTTP server via the
//! `BAML_AGENT_SKILLS_COMMITS_URL` / `BAML_AGENT_SKILLS_ARCHIVE_BASE_URL`
//! overrides, so the test exercises the real fetch/parse/install/record flow
//! without network access.

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn skill_archive() -> Vec<u8> {
    let content = "---\nname: baml-core\ndescription: test skill\n---\n# Core\n";
    let mut bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("baml-skill-{COMMIT}/skills/baml-core/SKILL.md"),
                content.as_bytes(),
            )
            .unwrap();
        builder.finish().unwrap();
    }
    bytes
}

/// Serve canned GitHub-shaped responses: commit JSON for `/commits/main`,
/// the tarball for `/archive/<commit>`. Returns the server's base URL.
fn spawn_stub_github() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();

            let (content_type, body): (&str, Vec<u8>) = if path.starts_with("/commits") {
                (
                    "application/json",
                    format!(r#"{{"sha":"{COMMIT}"}}"#).into_bytes(),
                )
            } else if path.starts_with("/archive/") {
                ("application/gzip", skill_archive())
            } else {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
                continue;
            };
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&body);
        }
    });
    base
}

#[test]
fn default_install_fetches_head_and_records_provenance() {
    let server = spawn_stub_github();
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args(["agent", "install", "--dir"])
        .arg(project.path())
        .env("BAML_HOME", home.path())
        .env(
            "BAML_AGENT_SKILLS_COMMITS_URL",
            format!("{server}/commits/main"),
        )
        .env(
            "BAML_AGENT_SKILLS_ARCHIVE_BASE_URL",
            format!("{server}/archive"),
        )
        .env_remove("BAML_AGENT_SKILLS_RELEASE_VERSION")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Skills installed into both agent directories.
    for dir in [".agents/skills", ".claude/skills"] {
        let skill = project.path().join(dir).join("baml-core/SKILL.md");
        let content = fs::read_to_string(&skill).unwrap();
        assert!(content.contains("name: baml-core"), "{content}");
    }

    // Provenance recorded in state.toml.
    let state = fs::read_to_string(home.path().join("state.toml")).unwrap();
    assert!(state.contains("[skills]"), "{state}");
    assert!(
        state.contains(&format!("installed_commit = \"{COMMIT}\"")),
        "{state}"
    );
    assert!(state.contains("installed_at = \""), "{state}");

    // Freshness cache updated to the installed commit, so the wrapper's
    // outdated warning clears immediately.
    let cache =
        fs::read_to_string(home.path().join("manifest-cache/skills/latest-commit.json")).unwrap();
    assert_eq!(cache, format!(r#"{{"sha":"{COMMIT}"}}"#));
}
