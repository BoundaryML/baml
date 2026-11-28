//! `baml cloud` management commands against stand-ins for the API endpoint,
//! which holds the login, and the control plane, which holds the resources.
use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
};

const ACCESS: &str = "Bearer bdry_access_test";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn login_session() -> Value {
    json!({"accessToken": "bdry_access_test", "refreshToken": "bdry_session_test",
        "expiresAt": now() + 300, "caller": {"userId": "user_1", "email": "a@example.test"}})
}

fn cloud_session() -> Value {
    json!({
        "user": {"id": "user_1", "email": "a@example.test"},
        "customerHosting": false,
        "organizations": [{
            "id": "org_1", "name": "Acme", "slug": "acme", "role": "admin",
            "projects": [{
                "id": "proj_1", "name": "app",
                "environments": [
                    {"id": "env_1", "name": "production", "kind": "production", "readiness": "ready"},
                ],
            }],
        }],
    })
}

fn dashboard(version: u64, name: &str) -> Value {
    json!({"id": "dsh_1", "projectId": "proj_1", "key": "latency", "latest": {
        "id": "dsv_1", "version": version, "name": name, "sha256": "ab", "size": 12,
        "createdBy": "user_1", "createdAt": 1_800_000_000,
    }})
}

struct Cloud {
    api: MockServer,
    control: MockServer,
    home: tempfile::TempDir,
}

impl Cloud {
    /// Both planes, with a login already saved: the API endpoint approves the
    /// device login at once and answers every refresh.
    async fn logged_in() -> Self {
        let cloud = Self::logged_out().await;
        Mock::given(path("/v1/auth/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "deviceCode": "dc_1", "userCode": "RRGQ-BJVS",
                "verificationUri": format!("{}/device", cloud.api.uri()),
                "expiresAt": now() + 300, "intervalSeconds": 1,
            })))
            .mount(&cloud.api)
            .await;
        Mock::given(path("/v1/auth/device/poll"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"status": "approved", "session": login_session()})),
            )
            .mount(&cloud.api)
            .await;
        Mock::given(path("/v1/auth/refresh"))
            .and(body_json(json!({"refreshToken": "bdry_session_test"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(login_session()))
            .mount(&cloud.api)
            .await;
        let login = cloud.baml(&["cloud", "auth", "login", "--no-open"]);
        assert!(login.status.success(), "{login:?}");
        Mock::given(method("GET"))
            .and(path("/v1/session"))
            .and(header("authorization", ACCESS))
            .respond_with(ResponseTemplate::new(200).set_body_json(cloud_session()))
            .mount(&cloud.control)
            .await;
        cloud
    }

    async fn logged_out() -> Self {
        Self {
            api: MockServer::start().await,
            control: MockServer::start().await,
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let home: &Path = self.home.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
        command
            .args(["--agent-skill-check", "off"])
            .args(args)
            .current_dir(home)
            .env_remove("BOUNDARY_API_KEY")
            .env_remove("BOUNDARY_PROJECT")
            .env("BAML_HOME", home)
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DO_NOT_TRACK", "1")
            .env("BOUNDARY_API_URL", self.api.uri())
            .env("BOUNDARY_CLOUD_SERVICES_URL", self.control.uri())
            .stdin(std::process::Stdio::null());
        command
    }

    fn baml(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("spawn baml-cli")
    }

    /// The control plane answers this request, sent with the access token, once.
    async fn answers(&self, verb: &str, at: &str, body: Option<Value>, answer: ResponseTemplate) {
        let mut mock = Mock::given(method(verb))
            .and(path(at))
            .and(header("authorization", ACCESS));
        if let Some(body) = body {
            mock = mock.and(body_json(body));
        }
        mock.respond_with(answer)
            .expect(1)
            .mount(&self.control)
            .await;
    }
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn stdout(output: &Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listings_are_tables_as_text_and_boundarys_json_as_json() {
    let cloud = Cloud::logged_in().await;
    assert_eq!(
        stdout(&cloud.baml(&["cloud", "org", "list"])),
        "HANDLE  NAME  ROLE   PROJECTS  ID\nacme    Acme  admin  1         org_1\n"
    );
    assert_eq!(
        stdout(&cloud.baml(&["cloud", "env", "list", "--project", "acme/app"])),
        "NAME        KIND        READINESS  ID\nproduction  production  ready      env_1\n"
    );

    let members = json!([{"id": "om_1", "userId": "user_2", "email": "b@example.test",
        "name": null, "role": "admin", "status": "active"}]);
    Mock::given(method("GET"))
        .and(path("/v1/organizations/org_1/members"))
        .and(header("authorization", ACCESS))
        .respond_with(ok(json!({"members": members, "invitations": []})))
        .expect(2)
        .mount(&cloud.control)
        .await;
    assert_eq!(
        stdout(&cloud.baml(&["cloud", "members", "list"])),
        "EMAIL           NAME  ROLE   STATUS  ID\nb@example.test  -     admin  active  om_1\n"
    );
    let json = stdout(&cloud.baml(&["cloud", "members", "list", "--output-format", "json"]));
    assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), members);

    // The login's refresh credential stays at the API endpoint.
    for request in cloud.control.received_requests().await.unwrap() {
        assert_eq!(request.headers["authorization"], ACCESS);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_become_ids_and_mutations_report_on_stderr() {
    let cloud = Cloud::logged_in().await;

    // The only organization is the default one.
    cloud
        .answers(
            "POST",
            "/v1/organizations/org_1/projects",
            Some(json!({"name": "web"})),
            ResponseTemplate::new(201)
                .set_body_json(json!({"id": "proj_2", "name": "web", "environments": []})),
        )
        .await;
    assert_eq!(
        stdout(&cloud.baml(&["cloud", "project", "create", "web"])),
        "Name: web\nID: proj_2\nEnvironments: none\n"
    );

    // An environment is named, and its ID is what the control plane gets.
    cloud
        .answers(
            "GET",
            "/v1/environments/env_1/hosting",
            None,
            ok(
                json!({"environmentId": "env_1", "name": "production", "hosting": "hosted",
                "readiness": "ready", "dataPlane": null}),
            ),
        )
        .await;
    assert_eq!(
        stdout(&cloud.baml(&["cloud", "env", "hosting", "production"])),
        "Name: production\nHosting: hosted\nReadiness: ready\nID: env_1\n"
    );

    // A member is named by email; removing one prints no result.
    Mock::given(method("GET"))
        .and(path("/v1/organizations/org_1/members"))
        .respond_with(ok(json!({"invitations": [], "members": [
            {"id": "om_1", "userId": "user_2", "email": "b@example.test", "name": "Bea",
                "role": "member", "status": "active"},
        ]})))
        .mount(&cloud.control)
        .await;
    cloud
        .answers(
            "POST",
            "/v1/organizations/org_1/members/om_1/remove",
            Some(json!({"revokeKeys": true})),
            ResponseTemplate::new(204),
        )
        .await;
    let removed = cloud.baml(&[
        "cloud",
        "members",
        "delete",
        "B@example.test",
        "--revoke-keys",
        "--output-format",
        "json",
    ]);
    assert_eq!(stdout(&removed), "");
    assert!(
        stderr(&removed).contains("b@example.test from Acme"),
        "{removed:?}"
    );

    // A rename replaces the version the dashboard is at now.
    Mock::given(method("GET"))
        .and(path("/v1/dashboards/dsh_1"))
        .and(query_param("limit", "1"))
        .respond_with(ok(
            json!({"dashboard": dashboard(4, "Latency"), "versions": []}),
        ))
        .expect(1)
        .mount(&cloud.control)
        .await;
    cloud
        .answers(
            "POST",
            "/v1/dashboards/dsh_1/rename",
            Some(json!({"name": "Latency by model", "expectedVersion": 4})),
            ok(dashboard(5, "Latency by model")),
        )
        .await;
    let renamed = stdout(&cloud.baml(&[
        "cloud",
        "dashboards",
        "update",
        "dsh_1",
        "--name",
        "Latency by model",
    ]));
    assert!(
        renamed.starts_with("Name: Latency by model\nKey: latency\nID: dsh_1\nVersion: 5\n"),
        "{renamed}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_name_fails_before_any_change() {
    let cloud = Cloud::logged_in().await;
    let output = cloud.baml(&["cloud", "project", "create", "web", "--org", "beta"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        stderr(&output).contains("you do not belong to an organization `beta`; yours: acme"),
        "{output:?}"
    );
    let requests = cloud.control.received_requests().await.unwrap();
    assert!(
        requests.iter().all(|request| request.method == "GET"),
        "{requests:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejection_is_worded_for_a_saved_login() {
    let cloud = Cloud::logged_in().await;
    cloud
        .answers(
            "GET",
            "/v1/organizations/org_1/billing",
            None,
            ResponseTemplate::new(403).set_body_json(json!({
                "code": "FORBIDDEN", "message": "Only administrators can see billing.",
                "retryable": false,
            })),
        )
        .await;
    let output = cloud.baml(&["cloud", "billing", "show"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = stderr(&output);
    for expected in [
        "Boundary denied the cloud request (HTTP 403).",
        "Only administrators can see billing.",
        "Ask an administrator of the organization for access",
        "Cloud command failed.",
    ] {
        assert!(
            stderr.contains(expected),
            "missing `{expected}` in:\n{stderr}"
        );
    }
    assert!(!stderr.contains("BOUNDARY_API_KEY"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn management_needs_a_login_and_a_control_plane_for_its_endpoint() {
    let cloud = Cloud::logged_out().await;

    // An API key is not a login.
    let output = cloud
        .command(&["cloud", "org", "list"])
        .env("BOUNDARY_API_KEY", "bdry_secret_not_a_login")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let message = stderr(&output);
    assert!(
        message.contains("No Boundary credential is available for the cloud request."),
        "{message}"
    );
    assert!(
        message.contains("Run `baml cloud auth login`."),
        "{message}"
    );
    assert!(!message.contains("BOUNDARY_API_KEY"), "{message}");

    // Only Boundary's own API endpoint has a default control plane.
    let output = cloud
        .command(&["cloud", "org", "list"])
        .env_remove("BOUNDARY_CLOUD_SERVICES_URL")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let message = stderr(&output);
    assert!(
        message
            .contains("Boundary services URL is required when the API URL is not Boundary's own"),
        "{message}"
    );
    assert!(
        message.contains("Set BOUNDARY_CLOUD_SERVICES_URL or boundary.services_url in baml.toml."),
        "{message}"
    );

    for server in [&cloud.api, &cloud.control] {
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
