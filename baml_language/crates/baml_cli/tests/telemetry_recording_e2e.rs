//! Real CLI roots and packed home roots must not accidentally follow cwd or
//! serialized build-machine source paths. Keep all output inside temp fixtures.
mod common;

use std::{path::Path, process::Command};

const RECORDINGS: &str = ".baml/btel/recordings";

fn cli(cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(common::baml_cli());
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("BAML_HOME", home.join(".baml"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .env("BAML_CACHE_DIR", home.join("cache"))
        .env("BAML_TELEMETRY", "medium")
        .env_remove("BOUNDARY_URL")
        .env_remove("BOUNDARY_API_KEY");
    command
}

fn run(command: &mut Command, success: bool) {
    let output = command.output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("telemetry recording failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_recordings(root: &Path, count: usize) {
    let directories: Vec<_> = std::fs::read_dir(root.join(RECORDINGS))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), count);
    for directory in directories {
        let recording = btel_file::read_directory(&directory).unwrap();
        assert!(!recording.files.is_empty(), "{}", directory.display());
        assert!(recording.issues.is_empty(), "{:?}", recording.issues);
        assert!(recording.files.iter().any(|file| {
            file.aggregates
                .as_ref()
                .is_some_and(|batch| !batch.entries.is_empty())
        }));
    }
}

fn home(root: &Path) -> std::path::PathBuf {
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".baml")).unwrap();
    std::fs::write(
        home.join(".baml/config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    home
}

#[test]
fn cli_uses_project_root_for_cold_cached_nested_expression_and_test_execution() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    common::write_project(
        &project,
        r#"
function main() -> int { 7 }
function fail() -> int { assert.is_true(false); 0 }
testset "smoke" { test "ok" { assert.is_true(true) } }
"#,
    );
    let home = home(temp.path());
    let nested = project.join("baml_src/nested");
    std::fs::create_dir(&nested).unwrap();
    // --from must win over both cwd and the baml_src subdirectory. Repeat to
    // cover a bytecode-cache hit as well as compilation.
    for count in 1..=2 {
        run(
            cli(temp.path(), &home)
                .args(["run", "main", "--from"])
                .arg(&project),
            true,
        );
        assert_recordings(&project, count);
    }
    run(cli(&nested, &home).args(["run", "main"]), true);
    assert_recordings(&project, 3);
    run(
        cli(temp.path(), &home)
            .args(["run", "-e", "1 + 2", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 4);
    run(
        cli(temp.path(), &home)
            .args(["test", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 5);
    run(
        cli(temp.path(), &home)
            .args(["run", "fail", "--from"])
            .arg(&project),
        false,
    );
    assert_recordings(&project, 6);
    run(
        cli(temp.path(), &home)
            .args(["run", "main", "--from"])
            .arg(&project)
            .env("BAML_TELEMETRY", "off"),
        true,
    );
    assert_recordings(&project, 6);
    assert!(!temp.path().join(RECORDINGS).exists());
    assert!(!nested.join(RECORDINGS).exists());
    assert!(!home.join(RECORDINGS).exists());
}

#[test]
fn packed_modes_write_to_user_home_even_when_launched_in_another_project() {
    let _built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let launch = temp.path().join("launch");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&launch).unwrap();
    common::write_project(&source, "function main() -> int { 9 }\n");
    common::write_project(&launch, "function main() -> int { 42 }\n");
    let home = home(temp.path());
    for (index, target) in [vec!["main"], vec!["-f", "main"]].iter().enumerate() {
        let binary = temp.path().join(format!("packed-{index}"));
        run(
            cli(temp.path(), &home)
                .args(["pack", "--from"])
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .args(target),
            true,
        );
        let mut command = Command::new(&binary);
        command
            .current_dir(&launch)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("BAML_TELEMETRY", "medium")
            .env_remove("BOUNDARY_URL")
            .env_remove("BOUNDARY_API_KEY");
        if index == 1 {
            command.arg("main");
        }
        run(&mut command, true);
        assert_recordings(&home, index + 1);
        run(command.env("BAML_TELEMETRY", "off"), true);
        assert_recordings(&home, index + 1);
    }
    assert!(!source.join(RECORDINGS).exists());
    assert!(!launch.join(RECORDINGS).exists());
    assert!(!temp.path().join(RECORDINGS).exists());
}

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

async fn cloud_server() -> wiremock::MockServer {
    use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

async fn assert_cloud_delivery(server: &wiremock::MockServer, host: &str, sources: bool) {
    use prost::Message as _;
    use sha2::{Digest, Sha256};
    let requests = server.received_requests().await.unwrap();
    let prepares: Vec<btel_bcs::wire::PrepareUploadsRequest> = requests
        .iter()
        .filter(|request| request.method == "POST")
        .map(|request| {
            assert!(request.url.path().starts_with("/tenant/v1/recordings/"));
            assert!(request.url.path().ends_with("/uploads:prepare"));
            assert_eq!(
                request.headers.get("authorization").unwrap(),
                "Bearer test-cloud-key"
            );
            serde_json::from_slice(&request.body).unwrap()
        })
        .collect();
    assert!(!prepares.is_empty());
    let mut recordings = 0;
    let mut captured_sources = false;
    let mut snapshots = 0;
    for request in requests.iter().filter(|request| request.method == "PUT") {
        assert!(request.headers.get("authorization").is_none());
        let envelope =
            btel_bcs::proto::CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
        assert_eq!(envelope.format_version, 1);
        if let Some(bytes) = envelope.recording_file {
            let file = btel_recorder::proto::RecordingFile::decode(bytes.as_slice()).unwrap();
            let header = file.header.unwrap();
            assert_eq!(header.host.as_deref(), Some(host));
            assert!(header.command.is_empty());
            captured_sources |= header.source_cas_id.is_some();
            let prepare = prepares
                .iter()
                .find(|prepare| {
                    prepare.recording.recording_file_sequence == file.sequence
                        && prepare.recording.recording_id == hex::encode(&header.recording_id)
                })
                .unwrap();
            assert_eq!(prepare.recording.encoded_length, bytes.len() as u64);
            assert_eq!(
                prepare.recording.sha256,
                hex::encode(Sha256::digest(&bytes))
            );
            recordings += 1;
        }
        for object in envelope.cas_objects {
            assert_eq!(object.blob_sha256, Sha256::digest(&object.blob).to_vec());
            let snapshot = btel_snapshot::decode_blob(&object.blob, &Default::default()).unwrap();
            assert_eq!(snapshot.id.as_bytes().as_slice(), object.snapshot_id);
            snapshots += 1;
        }
    }
    assert_eq!(recordings, prepares.len());
    if sources {
        assert!(captured_sources);
        assert!(snapshots > 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_routes_recordings_and_sources_to_the_configured_data_plane() {
    let server = cloud_server().await;
    let base = format!("{}/tenant/", server.uri());
    tokio::task::spawn_blocking(move || {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        common::write_project(&project, "function main() -> int { 7 }\n");
        let home = home(temp.path());
        run(
            cli(&project, &home)
                .args(["run", "main"])
                .env("BOUNDARY_URL", &base)
                .env("BOUNDARY_API_KEY", "test-cloud-key"),
            true,
        );
        assert!(!project.join(RECORDINGS).exists());
        assert!(!home.join(RECORDINGS).exists());
    })
    .await
    .unwrap();
    assert_cloud_delivery(&server, "baml", true).await;
}

#[test]
fn cloud_configuration_is_opt_in_and_telemetry_off_ignores_it() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    common::write_project(&project, "function main() -> int { 7 }\n");
    let home = home(temp.path());
    run(
        cli(&project, &home)
            .args(["run", "main"])
            .env("BOUNDARY_API_KEY", "existing-key"),
        true,
    );
    assert_recordings(&project, 1);
    run(
        cli(&project, &home)
            .args(["run", "main"])
            .env("BOUNDARY_URL", "invalid-private-url")
            .env("BOUNDARY_API_KEY", "private-key")
            .env("BAML_TELEMETRY", "off"),
        true,
    );
    assert_recordings(&project, 1);
    for key in [None, Some("private-key")] {
        let mut command = cli(&project, &home);
        command
            .args(["run", "main"])
            .env("BOUNDARY_URL", "invalid-private-url");
        if let Some(key) = key {
            command.env("BOUNDARY_API_KEY", key);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid cloud telemetry configuration"),
            "{stderr}"
        );
        assert!(!stderr.contains("invalid-private-url"));
        assert!(!stderr.contains("private-key"));
    }
    assert_recordings(&project, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packed_programs_select_the_data_plane_at_run_time() {
    let server = cloud_server().await;
    let base = format!("{}/tenant/", server.uri());
    tokio::task::spawn_blocking(move || {
        let _built = common::ensure_built();
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        common::write_project(&project, "function main() -> int { 7 }\n");
        let home = home(temp.path());
        for (index, target) in [vec!["main"], vec!["-f", "main"]].iter().enumerate() {
            let binary = temp.path().join(format!("cloud-packed-{index}"));
            run(
                cli(&project, &home)
                    .args(["pack", "-o"])
                    .arg(&binary)
                    .args(target),
                true,
            );
            let mut command = Command::new(&binary);
            command
                .current_dir(temp.path())
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env("BAML_TELEMETRY", "medium")
                .env("BOUNDARY_URL", &base)
                .env("BOUNDARY_API_KEY", "test-cloud-key");
            if index == 1 {
                command.arg("main");
            }
            run(&mut command, true);
        }
        assert!(!project.join(RECORDINGS).exists());
        assert!(!home.join(RECORDINGS).exists());
    })
    .await
    .unwrap();
    assert_cloud_delivery(&server, "pack", false).await;
}
