#![allow(clippy::doc_markdown)]

//! Project-id / quota-project resolution — mirrors `google-auth`:
//!
//!     GOOGLE_CLOUD_PROJECT > GCLOUD_PROJECT (legacy) > GAC credential file >
//!     active gcloud configuration's core.project > well-known ADC file >
//!     GCE metadata server
//!
//! and for the quota project (the `x-goog-user-project` header):
//!
//!     GOOGLE_CLOUD_QUOTA_PROJECT > GAC credential file > well-known ADC file

mod common;

use common::MockIo;
use forked_google_cloud_auth::{HttpResponse, project_id, quota_project_id};

const WELL_KNOWN: &str = "/home/dev/.config/gcloud/application_default_credentials.json";
const METADATA_PROJECT_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/project/project-id";

fn home() -> MockIo {
    MockIo::new().env("HOME", "/home/dev")
}

#[tokio::test]
async fn env_project_wins_over_everything() {
    let io = home()
        .env("GOOGLE_CLOUD_PROJECT", "env-project")
        .env("GCLOUD_PROJECT", "legacy-project")
        .file(WELL_KNOWN, r#"{"quota_project_id":"adc-project"}"#);
    assert_eq!(project_id(&io).await, Some("env-project".to_string()));
}

#[tokio::test]
async fn legacy_gcloud_project_env_is_honored() {
    let io = home().env("GCLOUD_PROJECT", "legacy-project");
    assert_eq!(project_id(&io).await, Some("legacy-project".to_string()));
}

#[tokio::test]
async fn unexpanded_dollar_placeholder_is_ignored() {
    // `.env` files sometimes carry literal `$VAR` values; skip them.
    let io = home()
        .env("GOOGLE_CLOUD_PROJECT", "$MY_PROJECT")
        .env("GCLOUD_PROJECT", "legacy-project");
    assert_eq!(project_id(&io).await, Some("legacy-project".to_string()));
}

#[tokio::test]
async fn gac_credential_file_supplies_project() {
    let io = home()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/sa.json")
        .file(
            "/sa.json",
            r#"{"type":"service_account","project_id":"sa-project"}"#,
        );
    assert_eq!(project_id(&io).await, Some("sa-project".to_string()));
}

#[tokio::test]
async fn gcloud_config_core_project_is_read_from_disk() {
    // No env project, no GAC: the active gcloud configuration supplies it —
    // read from the config file, never by shelling out to `gcloud`.
    let io = home()
        .file("/home/dev/.config/gcloud/active_config", "work\n")
        .file(
            "/home/dev/.config/gcloud/configurations/config_work",
            "[core]\naccount = dev@example.com\nproject = gcloud-config-project\n",
        );
    assert_eq!(
        project_id(&io).await,
        Some("gcloud-config-project".to_string())
    );
}

#[tokio::test]
async fn cloudsdk_active_config_name_overrides_active_config_file() {
    let io = home()
        .env("CLOUDSDK_ACTIVE_CONFIG_NAME", "override")
        .file("/home/dev/.config/gcloud/active_config", "work\n")
        .file(
            "/home/dev/.config/gcloud/configurations/config_override",
            "[core]\nproject = override-project\n",
        );
    assert_eq!(project_id(&io).await, Some("override-project".to_string()));
}

#[tokio::test]
async fn well_known_adc_quota_project_is_a_fallback() {
    let io = home().file(WELL_KNOWN, r#"{"quota_project_id":"adc-quota-project"}"#);
    assert_eq!(project_id(&io).await, Some("adc-quota-project".to_string()));
}

#[tokio::test]
async fn metadata_server_is_the_last_resort() {
    let io = home().http(|method, url, headers, _b| {
        assert_eq!(method, "GET");
        assert_eq!(url, METADATA_PROJECT_URL);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("Metadata-Flavor") && v == "Google")
        );
        Ok(HttpResponse {
            status: 200,
            body: "metadata-project\n".to_string(),
        })
    });
    assert_eq!(project_id(&io).await, Some("metadata-project".to_string()));
}

#[tokio::test]
async fn no_project_anywhere_is_none() {
    let io = home().http(|_m, _u, _h, _b| {
        Ok(HttpResponse {
            status: 404,
            body: String::new(),
        })
    });
    assert_eq!(project_id(&io).await, None);
}

// --- quota project -----------------------------------------------------------

#[tokio::test]
async fn quota_project_env_wins() {
    let io = home()
        .env("GOOGLE_CLOUD_QUOTA_PROJECT", "env-quota")
        .file(WELL_KNOWN, r#"{"quota_project_id":"adc-quota"}"#);
    assert_eq!(quota_project_id(&io).await, Some("env-quota".to_string()));
}

#[tokio::test]
async fn quota_project_from_gac_then_well_known() {
    let gac = home()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/user.json")
        .file(
            "/user.json",
            r#"{"type":"authorized_user","quota_project_id":"gac-quota"}"#,
        )
        .file(WELL_KNOWN, r#"{"quota_project_id":"adc-quota"}"#);
    assert_eq!(quota_project_id(&gac).await, Some("gac-quota".to_string()));

    let well_known = home().file(WELL_KNOWN, r#"{"quota_project_id":"adc-quota"}"#);
    assert_eq!(
        quota_project_id(&well_known).await,
        Some("adc-quota".to_string())
    );

    let none = home();
    assert_eq!(quota_project_id(&none).await, None);
}
