use std::process::Command;

use super::{BOUNDARY_API_KEY, BOUNDARY_URL, Destination, TelemetryRecording};

#[test]
fn from_boundary_env() {
    const CASE: &str = "BAML_BOUNDARY_ENV_TEST_CASE";
    const URL: &str = "https://example.invalid/publisher/";
    const KEY: &str = "bml_test_key";
    let cases = [
        (Some(URL), Some(KEY), true),
        (None, None, false),
        (None, Some(KEY), false),
        (Some(URL), None, false),
        (Some(""), Some(KEY), false),
        (Some(URL), Some(""), false),
        (Some(""), Some(""), false),
        (Some("not a URL"), Some(KEY), false),
        // Validation belongs to delivery, not the environment constructor.
        (Some("http://localhost:1234/"), Some(KEY), true),
        (
            Some("https://user:pass@example.invalid/?q=1#f"),
            Some(KEY),
            true,
        ),
        (Some(URL), Some("invalid\nheader"), true),
    ];
    let Ok(case) = std::env::var(CASE) else {
        for (index, (url, key, _)) in cases.iter().enumerate() {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "telemetry::boundary_env_tests::from_boundary_env",
                "--nocapture",
            ]);
            command.env(CASE, index.to_string());
            command
                .env_remove(BOUNDARY_URL)
                .env_remove(BOUNDARY_API_KEY);
            if let Some(url) = url {
                command.env(BOUNDARY_URL, url);
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
    let recording = TelemetryRecording::from_boundary_env();
    assert_eq!(recording.is_some(), enabled);
    if let Some(recording) = recording {
        let Destination::Cloud { delivery, .. } = recording.destination else {
            panic!("Boundary environment must select cloud delivery");
        };
        assert_eq!(delivery.prepare_base_url.as_str(), url.unwrap());
        assert_eq!(delivery.bearer_token.as_deref(), key);
    }
}
