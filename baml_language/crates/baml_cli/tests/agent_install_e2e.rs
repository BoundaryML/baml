//! End-to-end test for `baml-cli agent install`'s default path: download the
//! skill repo's main-branch tarball, install the skills, and record the
//! commit embedded in the tarball's pax global header as provenance in
//! `state.toml`.
//!
//! The tarball host is stubbed with a local HTTP server via the
//! `BAML_AGENT_SKILLS_ARCHIVE_BASE_URL` override. Deliberately, no GitHub
//! commits API stub exists: the default install must never touch the REST
//! API (whose unauthenticated rate limit used to hard-block installs).

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A codeload-shaped tarball: pax global header carrying the source commit
/// (as `git archive` writes it), then the skill files.
fn skill_archive(pax_comment: Option<&str>) -> Vec<u8> {
    let content = "---\nname: baml-core\ndescription: test skill\n---\n# Core\n";
    let mut bytes = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        if let Some(comment) = pax_comment {
            let record_content = format!("comment={comment}\n");
            let mut total = record_content.len();
            // The length prefix counts itself: grow until it stabilizes.
            loop {
                let with_prefix = total.to_string().len() + 1 + record_content.len();
                if with_prefix == total {
                    break;
                }
                total = with_prefix;
            }
            let record = format!("{total} {record_content}");
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::XGlobalHeader);
            header.set_size(record.len() as u64);
            header.set_mode(0o666);
            header.set_cksum();
            builder
                .append_data(&mut header, "pax_global_header", record.as_bytes())
                .unwrap();
        }
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "baml-skill-main/skills/baml-core/SKILL.md",
                content.as_bytes(),
            )
            .unwrap();
        builder.finish().unwrap();
    }
    bytes
}

/// Serve the tarball at `/archive/<ref>`; everything else (including any
/// attempt to hit a commits API) 404s. Returns the server's base URL.
fn spawn_stub_codeload(archive: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();

            if !path.starts_with("/archive/") {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
                continue;
            }
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/gzip\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    archive.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&archive);
        }
    });
    base
}

fn install_command(server: &str, home: &std::path::Path, dir: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    command
        .args(["agent", "install", "--dir"])
        .arg(dir)
        .env("BAML_HOME", home)
        .env(
            "BAML_AGENT_SKILLS_ARCHIVE_BASE_URL",
            format!("{server}/archive"),
        )
        // The default install must work for a brand-new user with no GitHub
        // credentials of any kind.
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN");
    command
}

#[test]
fn default_install_needs_no_github_api_and_records_pax_provenance() {
    let server = spawn_stub_codeload(skill_archive(Some(COMMIT)));
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();

    let output = install_command(&server, home.path(), project.path())
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

    // Provenance recorded in state.toml from the tarball's pax header.
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

#[test]
fn headerless_archive_installs_without_provenance() {
    let server = spawn_stub_codeload(skill_archive(None));
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();

    let output = install_command(&server, home.path(), project.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let skill = project.path().join(".claude/skills/baml-core/SKILL.md");
    assert!(skill.is_file());

    // No commit identity → no [skills] provenance recorded.
    let state = fs::read_to_string(home.path().join("state.toml")).unwrap_or_default();
    assert!(!state.contains("[skills]"), "{state}");
}

#[test]
fn unreachable_archive_fails_with_from_hint() {
    let server = spawn_stub_codeload(skill_archive(Some(COMMIT)));
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();

    let mut command = install_command(&server, home.path(), project.path());
    command.env(
        "BAML_AGENT_SKILLS_ARCHIVE_BASE_URL",
        format!("{server}/missing"),
    );
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--from"), "{stderr}");
}
