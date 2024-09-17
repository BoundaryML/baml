mod harness;

use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use eventsource_stream::Eventsource;
use futures::stream::StreamExt;
use harness::Harness;
use http::StatusCode;
use indoc::indoc;
use rstest::rstest;
use scopeguard::defer;
use serde_json::json;

#[cfg(not(feature = "skip-integ-tests"))]
mod test_cli {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    fn init() -> Result<()> {
        let h = Harness::new("init_test")?;

        let run = h.run_cli("init")?.output()?;
        assert_eq!(run.status.code(), Some(0));

        Ok(())
    }

    // NB(sam): this test is flaky due to how the port reservation works. harness needs to be updated to choose ports for test cases.
    #[rstest]
    #[tokio::test]
    async fn cli(#[values("dev", "serve")] cmd: &str) -> Result<()> {
        let h = Harness::new(format!("{cmd}_test"))?;

        const PORT: &str = "2024";

        let run = h.run_cli("init")?.output()?;
        assert_eq!(run.status.code(), Some(0));

        let run = h.run_cli(cmd)?.output()?;
        assert_ne!(run.status.code(), Some(0));
        assert!(run.stdout.is_empty());
        assert!(String::from_utf8(run.stderr)?.contains("Please run with --preview"),);

        let mut child = h
            .run_cli(format!("{cmd} --preview --port {PORT}"))?
            .spawn()?;
        defer! {
            let _ = child.kill();
            std::thread::sleep(Duration::from_secs(1));
        }

        assert!(
            reqwest::get(&format!("http://localhost:{PORT}/_debug/ping"))
                .await?
                .status()
                .is_success()
        );

        let resume = indoc! {"
      Vaibhav Gupta
      vbv@boundaryml.com

      Experience:
      - Founder at BoundaryML
      - CV Engineer at Google
      - CV Engineer at Microsoft

      Skills:
      - Rust
      - C++
    "};
        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert!(resp.status().is_success());
        let resp_json = resp.json::<serde_json::Value>().await?;
        assert_eq!(resp_json["name"], "Vaibhav Gupta");

        let stream_start = Instant::now();
        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/stream/ExtractResume"))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert!(resp.status().is_success());
        let resp = resp
            .bytes_stream()
            .eventsource()
            .map(|event| match event {
                Ok(event) => Ok((
                    serde_json::from_str::<serde_json::Value>(&event.data)?,
                    stream_start.elapsed(),
                )),
                Err(e) => Err(anyhow::Error::from(e)),
            })
            .collect::<Vec<_>>()
            .await;
        let resp = resp.into_iter().collect::<Result<Vec<_>>>()?;

        assert!(resp.len() > 2);
        let (_, time_to_first) = resp[0].clone();
        let (last_data, time_to_last) = resp.last().context("No last data")?.clone();
        assert!(
            // This is a funky assertion, but the tldr is that it's our heuristic that streaming is working.
            // Specifically, we're saying that:
            //
            //   - if the time to the first streamed response is fast, then the time
            //      to the last streamed response will also be fast
            //
            //   - if the time to the first streamed response is slow, then the time
            //      to the last streamed response should be at least 500ms later
            time_to_last - time_to_first > std::cmp::min(Duration::from_millis(500), time_to_first),
            "time-to-first={:?}, time-to-last={:?}",
            time_to_first,
            time_to_last
        );
        assert_eq!(last_data["name"], "Vaibhav Gupta");

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn serve_respects_baml_password() -> Result<()> {
        let h = Harness::new(format!("serve_password_test"))?;

        const PORT: &str = "2025";

        let run = h.run_cli("init")?.output()?;
        assert_eq!(run.status.code(), Some(0));

        let mut child = h
            .run_cli(format!("serve --preview --port {PORT}"))?
            .env("BAML_PASSWORD", "my-super-secret-password")
            .spawn()?;
        defer! { let _ = child.kill(); }

        assert!(
            reqwest::get(&format!("http://localhost:{PORT}/_debug/ping"))
                .await?
                .status()
                .is_success()
        );

        assert_eq!(
            reqwest::get(&format!("http://localhost:{PORT}/_debug/status"))
                .await?
                .status(),
            StatusCode::FORBIDDEN
        );

        let resume = indoc! {"
      Vaibhav Gupta
      vbv@boundaryml.com

      Experience:
      - Founder at BoundaryML
      - CV Engineer at Google
      - CV Engineer at Microsoft

      Skills:
      - Rust
      - C++
    "};
        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(resp.text().await?, "No authorization metadata\n");

        let resp = reqwest::Client::new()
            .post(&format!(
                "http://baml:wrong-password@localhost:{PORT}/call/ExtractResume"
            ))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            resp.text().await?,
            "Incorrect password provided in basic auth\n"
        );

        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .header("x-baml-api-key", "my-super-secret-password")
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.text().await?.starts_with("{"));

        let resp = reqwest::Client::new()
            .post(&format!(
                "http://baml:my-super-secret-password@localhost:{PORT}/call/ExtractResume"
            ))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.text().await?.starts_with("{"));

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn cli_fails_if_port_unavailable(#[values("dev", "serve")] cmd: &str) -> Result<()> {
        let h = Harness::new(format!("{cmd}_port_unavailable_test"))?;

        const PORT: &str = "2026";

        let run = h.run_cli("init")?.output()?;
        assert_eq!(run.status.code(), Some(0));

        let mut child = h
            .run_cli(format!("{cmd} --preview --port {PORT}"))?
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        defer! { let _ = child.kill(); }

        let run = h
            .run_cli(format!("{cmd} --preview --port {PORT}"))?
            .output()?;
        assert_ne!(run.status.code(), Some(0));
        assert!(String::from_utf8(run.stderr)?.contains("Address already in use"));

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn call_function_error_codes() -> Result<()> {
        let h = Harness::new(format!("invalid_arg_test"))?;

        const PORT: &str = "2035";

        let run = h.run_cli("init")?.output()?;
        assert_eq!(run.status.code(), Some(0));

        let mut child = h
            .run_cli(format!("serve --preview --port {PORT}"))?
            .spawn()?;
        defer! { let _ = child.kill(); }

        assert!(
            reqwest::get(&format!("http://localhost:{PORT}/_debug/ping"))
                .await?
                .status()
                .is_success()
        );

        let resume = indoc! {"
      Vaibhav Gupta
      vbv@boundaryml.com

      Experience:
      - Founder at BoundaryML
      - CV Engineer at Google
      - CV Engineer at Microsoft

      Skills:
      - Rust
      - C++
    "};
        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .json(&json!({ "not-resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = reqwest::Client::new()
            .post(&format!("http://localhost:{PORT}/call/ExtractResume"))
            .json(&json!({ "resume": { "not": "string" } }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = reqwest::Client::new()
            .post(&format!(
                "http://localhost:{PORT}/call/ExtractResumeNonexistent"
            ))
            .json(&json!({ "resume": resume }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        Ok(())
    }

    // #[rstest]
    // #[tokio::test]
    // async fn stream_function_error_codes() -> Result<()> {
    //     let h = Harness::new(format!("invalid_arg_test"))?;

    //     const PORT: &str = "2035";

    //     let run = h.run_cli("init")?.output()?;
    //     assert_eq!(run.status.code(), Some(0));

    //     let mut child = h
    //         .run_cli(format!("serve --preview --port {PORT}"))?
    //         .spawn()?;
    //     defer! { let _ = child.kill(); }

    //     assert!(
    //         reqwest::get(&format!("http://localhost:{PORT}/_debug/ping"))
    //             .await?
    //             .status()
    //             .is_success()
    //     );

    //     let resume = indoc! {"
    //   Vaibhav Gupta
    //   vbv@boundaryml.com

    //   Experience:
    //   - Founder at BoundaryML
    //   - CV Engineer at Google
    //   - CV Engineer at Microsoft

    //   Skills:
    //   - Rust
    //   - C++
    // "};
    //     let stream_start = Instant::now();
    //     let resp = reqwest::Client::new()
    //         .post(&format!("http://localhost:{PORT}/stream/ExtractResume"))
    //         .json(&json!({ "resume": resume }))
    //         .send()
    //         .await?;
    //     assert_eq!(resp.status(), StatusCode::OK);

    //     let resp = resp
    //         .bytes_stream()
    //         .eventsource()
    //         .map(|event| match event {
    //             Ok(event) => Ok((
    //                 serde_json::from_str::<serde_json::Value>(&event.data)?,
    //                 stream_start.elapsed(),
    //             )),
    //             Err(e) => Err(anyhow::Error::from(e)),
    //         })
    //         .collect::<Vec<_>>()
    //         .await;
    //     let resp = resp.into_iter().collect::<Result<Vec<_>>>()?;
    //     assert!(resp
    //         .last()
    //         .context("Stream should have at least 1 event")?
    //         .0["name"]
    //         .as_str()
    //         .context("retval.name should be a string")?
    //         .contains("Vaibhav"));

    //     let resp = reqwest::Client::new()
    //         .post(&format!("http://localhost:{PORT}/stream/ExtractResume"))
    //         .json(&json!({ "not-resume": resume }))
    //         .send()
    //         .await?;
    //     let stream_start = Instant::now();
    //     let resp = resp
    //         .bytes_stream()
    //         .eventsource()
    //         .map(|event| match event {
    //             Ok(event) => Ok((
    //                 serde_json::from_str::<serde_json::Value>(&event.data)?,
    //                 stream_start.elapsed(),
    //             )),
    //             Err(e) => Err(anyhow::Error::from(e)),
    //         })
    //         .collect::<Vec<_>>()
    //         .await;
    //     let resp = resp.into_iter().collect::<Result<Vec<_>>>()?;
    //     assert_eq!(resp, vec![]);
    //     // assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    //     // let resp = reqwest::Client::new()
    //     //     .post(&format!("http://localhost:{PORT}/stream/ExtractResume"))
    //     //     .json(&json!({ "resume": { "not": "string" } }))
    //     //     .send()
    //     //     .await?;
    //     // // assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    //     // let resp = reqwest::Client::new()
    //     //     .post(&format!(
    //     //         "http://localhost:{PORT}/stream/ExtractResumeNonexistent"
    //     //     ))
    //     //     .json(&json!({ "resume": resume }))
    //     //     .send()
    //     //     .await?;
    //     // // assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    //     // assert_eq!(resp.text().await?, "Function not found\n");

    //     Ok(())
    // }
}
