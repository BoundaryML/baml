use std::{process::Command, sync::Arc};

use btel_types::context::Context;

use super::{BOUNDARY_API_KEY, Destination, TelemetryRecording};

#[test]
fn from_boundary_env() {
    const CASE: &str = "BAML_BOUNDARY_ENV_TEST_CASE";
    const URL: &str = "https://example.invalid/publisher/";
    const KEY: &str = "bml_test_key";
    let cases = [
        (Some(URL), Some(KEY), true),
        (None, Some(KEY), true),
        (Some(URL), None, false),
        (Some("not a URL"), None, true),
        (Some("http://example.invalid/"), None, true),
        (Some(""), Some(KEY), true),
        (Some("  "), Some(KEY), true),
        (Some(URL), Some(""), false),
        (Some(""), Some(""), false),
        (Some("not a URL"), Some(KEY), true),
        (Some("http://example.invalid/"), Some(KEY), true),
        // Endpoint validation happens before discovering credentials.
        (Some("http://localhost:1234/"), Some(KEY), true),
        (
            Some("https://user:pass@example.invalid/?q=1#f"),
            Some(KEY),
            true,
        ),
        (Some(URL), Some("invalid\nheader"), true),
        (Some(URL), Some("local"), true),
        (Some("not a URL"), Some("local"), true),
    ];
    let Some(case) = baml_env::raw_var(CASE) else {
        for (index, (url, key, _)) in cases.iter().enumerate() {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "telemetry::boundary_env_tests::from_boundary_env",
                "--nocapture",
            ]);
            command.env(CASE, index.to_string());
            command
                .env_remove(BOUNDARY_API_KEY)
                .env_remove("BOUNDARY_API_URL")
                .env_remove("BOUNDARY_PROJECT")
                .env(
                    "BAML_HOME",
                    format!(
                        "/tmp/baml-boundary-env-tests-{}-{index}",
                        std::process::id()
                    ),
                );
            if let Some(url) = url {
                command.env("BOUNDARY_API_URL", url);
            }
            if let Some(key) = key {
                command.env(BOUNDARY_API_KEY, key);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "case {index}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        return;
    };

    let (url, key, enabled) = cases[case.parse::<usize>().unwrap()];
    let recording = if url == Some(URL) && key != Some("local") {
        // The supported endpoint override wins over the manifest default.
        TelemetryRecording::from_boundary_defaults(None, Some("not a URL"))
    } else {
        TelemetryRecording::from_boundary_env()
    };
    assert_eq!(recording.is_some(), enabled);
    if let Some(recording) = recording {
        if key == Some("local") {
            assert!(matches!(recording.destination, Destination::UserFiles));
            assert!(TelemetryRecording::from_boundary_defaults(None, url).is_none());
            return;
        }
        if matches!(recording.destination, Destination::InvalidConfiguration(_)) {
            let Err(error) = recording.start(None, Arc::default(), &Context::default()) else {
                panic!("invalid Boundary configuration must stop execution");
            };
            let crate::EngineError::CloudAuthorization(diagnostic) = error else {
                panic!("invalid Boundary configuration must use the shared diagnostic");
            };
            assert_eq!(diagnostic.kind.code(), "BOUNDARY_CONFIG_INVALID");
            let reason = if url == Some("not a URL") {
                "Boundary API URL must be a valid absolute URL"
            } else if url == Some("http://example.invalid/") {
                "Boundary API URL must use HTTPS, or HTTP on loopback"
            } else {
                "Boundary API URL must be a base URL without credentials, query or fragment"
            };
            assert_eq!(
                diagnostic.to_string(),
                format!(
                    r#"Boundary configuration is invalid: {reason}.

  To continue, choose one:
    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
                )
            );
            return;
        }
        let Destination::Cloud { delivery, .. } = recording.destination else {
            panic!("Boundary environment must select cloud delivery");
        };
        assert_eq!(
            delivery.prepare_base_url.as_str().trim_end_matches('/'),
            url.filter(|url| !url.trim().is_empty())
                .unwrap_or(bcs_api::auth::DEFAULT_API_URL)
                .trim_end_matches('/')
        );
        let authorization = delivery.authorization.as_ref().unwrap();
        assert_eq!(Some(authorization.authentication.bearer().expose()), key);
        assert!(delivery.bearer_token.is_none());
    }
}
