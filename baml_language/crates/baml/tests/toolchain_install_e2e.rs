use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};

fn baml_command(project: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml"));
    command
        .arg("toolchain")
        .arg("install")
        .current_dir(project.path())
        .env("BAML_HOME", project.path().join(".baml-home"))
        .env("HOME", project.path())
        .env_remove("BAML_VERSION");
    command
}

fn serve_manifest(version: &str, channel: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let artifacts = baml_release::SUPPORTED_RELEASE_TARGETS
        .iter()
        .map(|target| {
            format!(
                r#""{target}":{{"url":"https://example.com/{target}.tar.gz","sha256":"{}"}}"#,
                "a".repeat(64)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        r#"{{"schema":1,"version":"{version}","channel":"{channel}","released_at":"2026-08-13T00:00:00Z","artifacts":{{{artifacts}}}}}"#
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        let bytes_read = stream.read(&mut request).unwrap();
        assert!(bytes_read > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    (format!("http://{address}/manifest"), server)
}

#[test]
fn bare_install_uses_project_toolchain_pin() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[toolchain]\nversion = \"0.15.0\"\n",
    )
    .unwrap();

    let output = baml_command(&project)
        .args(["--manifest-base-url", "http://127.0.0.1:1/manifest"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("http://127.0.0.1:1/manifest/version/0.15.0.json"),
        "{stderr}"
    );
    assert!(!stderr.contains("usage:"), "{stderr}");
}

#[test]
fn bare_install_records_project_channel_resolution() {
    let project = tempfile::tempdir().unwrap();
    let baml_home = project.path().join(".baml-home");
    fs::write(
        project.path().join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[toolchain]\nchannel = \"canary\"\n",
    )
    .unwrap();
    let installed = baml_home.join("toolchains/0.15.0");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("VERSION"), "0.15.0\n").unwrap();
    let (manifest_base_url, server) = serve_manifest("0.15.0", "canary");

    let output = baml_command(&project)
        .args(["--manifest-base-url", &manifest_base_url])
        .output()
        .unwrap();
    server.join().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state = fs::read_to_string(baml_home.join("state.toml")).unwrap();
    assert!(state.contains("[channels.canary]"), "{state}");
    assert!(state.contains("active_version = \"0.15.0\""), "{state}");
}

#[test]
fn install_help_is_not_parsed_as_a_version() {
    let project = tempfile::tempdir().unwrap();
    let output = baml_command(&project)
        .args([
            "--help",
            "--manifest-base-url",
            "http://127.0.0.1:1/manifest",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("baml toolchain install [canary|nightly|version] [--force]"),
        "{stdout}"
    );
    assert!(!stdout.contains("127.0.0.1"), "{stdout}");
}

#[test]
fn bare_install_without_a_project_pin_is_actionable() {
    let project = tempfile::tempdir().unwrap();
    let output = baml_command(&project).output().unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "no BAML toolchain is pinned in baml.toml\nRun: baml toolchain install <canary|nightly|version>\n"
    );
}
