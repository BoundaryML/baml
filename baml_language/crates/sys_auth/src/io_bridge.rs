//! Bridges the forks' IO traits to BAML's [`RuntimeIo`].
//!
//! One adapter implements both `google_cloud_auth::TokenIo` and
//! `aws_config::CredentialIo`: they need the same three capabilities (env,
//! file read, HTTP), differing only in whether the request carries a body.
//! Routing them through `RuntimeIo` keeps credential resolution inside BAML's
//! sandbox — the host decides what env vars and paths are visible.
//!
//! # Trust boundary: `credential_process`
//!
//! One AWS capability has no `RuntimeIo` analogue. `credential_process` in an
//! AWS config profile names a command whose stdout is the credential document,
//! and AWS defines it as an *arbitrary* command — quoting or allow-listing the
//! string does not make it safe, because running the named program is the
//! entire feature. Executing it therefore steps outside the sandbox: a config
//! file that `fs_open` is allowed to read would otherwise become host code
//! execution that no embedder can deny.
//!
//! Rather than half-plumb a process capability through `RuntimeIo`, the
//! execution is **off by default and gated on an explicit opt-in**,
//! `BAML_AWS_CREDENTIAL_PROCESS=1`, read through `RuntimeIo::env_get` so the
//! host's own env policy is what grants it. When the variable is unset,
//! [`BamlAuthIo::run_command`] refuses with a `ConfigError::Io` naming the
//! variable and credential resolution moves on to the next provider in the
//! chain. The command is also run WITHOUT a shell (see [`split_command`]),
//! matching what the AWS SDKs do and keeping the argument vector out of a
//! second round of shell parsing.
//!
//! Known limitation: when the opt-in IS set, the subprocess runs with the host
//! process's full privileges and its own unrestricted IO. `RuntimeIo` cannot
//! observe or constrain it.

use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use sys_types::{BexExternalValue, runtime_io::RuntimeIo};

/// Total deadline for one credential HTTP round trip, in nanoseconds (the unit
/// `RuntimeIo::http__send` takes).
///
/// It must be bounded. The comment this replaced argued that credential
/// endpoints fast-fail, which is true of the token and container endpoints but
/// NOT of EC2 IMDS: on a host that is not an EC2 instance, connecting to
/// `169.254.169.254` typically hangs until the OS-level TCP timeout, and every
/// signing call runs through credential resolution, so one stalled probe blocks
/// the caller. The AWS SDKs bound IMDS at about a second for this reason; five
/// seconds is the same guarantee with headroom for a real token exchange over a
/// slow link.
const CREDENTIAL_REQUEST_TIMEOUT_NANOS: i64 = 5_000_000_000;

/// The env var that opts a host in to running `credential_process` commands.
///
/// Native-only: `run_command` cannot spawn a subprocess on wasm, so there is
/// nothing there to opt in to.
#[cfg(not(target_arch = "wasm32"))]
const CREDENTIAL_PROCESS_OPT_IN: &str = "BAML_AWS_CREDENTIAL_PROCESS";

/// Split a `credential_process` command line into program + arguments, honoring
/// single and double quotes and backslash escapes, without invoking a shell.
///
/// This is what lets the command run through `Command::new(program).args(..)`
/// instead of `sh -c <string>`: the OS never re-parses the string, so nothing in
/// a config file can smuggle in a second command through `;` or `$(…)`. It is
/// the same treatment botocore gives the option (`shlex.split`, then spawn
/// without a shell). Returns `None` when the command is blank or a quote is
/// left open.
///
/// Native-only, like the spawn it feeds.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn split_command(command: &str) -> Option<Vec<String>> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut has_current = false;
    let mut quote: Option<char> = None;
    let mut chars = command.chars();

    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    current.push(c);
                }
            }
            Some(_) => match c {
                '"' => quote = None,
                '\\' => match chars.next() {
                    // Inside double quotes a backslash only escapes these.
                    Some(next @ ('"' | '\\' | '$' | '`')) => current.push(next),
                    Some(next) => {
                        current.push('\\');
                        current.push(next);
                    }
                    None => return None,
                },
                _ => current.push(c),
            },
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    has_current = true;
                }
                '\\' => match chars.next() {
                    Some(next) => {
                        current.push(next);
                        has_current = true;
                    }
                    None => return None,
                },
                c if c.is_whitespace() => {
                    if has_current {
                        parts.push(std::mem::take(&mut current));
                        has_current = false;
                    }
                }
                _ => {
                    current.push(c);
                    has_current = true;
                }
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if has_current {
        parts.push(current);
    }
    if parts.is_empty() { None } else { Some(parts) }
}

pub(crate) struct BamlAuthIo {
    pub(crate) io: Arc<dyn RuntimeIo>,
}

impl BamlAuthIo {
    async fn env_var(&self, key: &str) -> Option<String> {
        self.io.env_get(key.to_string()).await.ok().flatten()
    }

    async fn file_text(&self, path: &str) -> Option<String> {
        let handle = self
            .io
            .fs_open(path.to_string(), BexExternalValue::String("r".into()))
            .await
            .ok()?;
        self.io.fs_file_text(&handle).await.ok()
    }

    /// One HTTP round trip through the runtime, returning `(status, body)`.
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: String,
    ) -> Result<(u16, String), String> {
        let mut header_map = IndexMap::new();
        for (k, v) in headers {
            header_map.insert(k.clone(), v.clone());
        }
        let request = sys_types::generated::owned::http::Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: header_map,
            body,
        };
        let resp = self
            .io
            .http__send(
                request,
                Arc::new(num_bigint::BigInt::from(CREDENTIAL_REQUEST_TIMEOUT_NANOS)),
            )
            .await
            .map_err(|e| e.to_string())?;
        let text = self
            .io
            .http_response_text(&resp)
            .await
            .map_err(|e| e.to_string())?;
        Ok((u16::try_from(resp.status_code).unwrap_or(0), text))
    }
}

#[async_trait]
impl google_cloud_auth::TokenIo for BamlAuthIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env_var(key).await
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        self.file_text(path).await
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<google_cloud_auth::HttpResponse, google_cloud_auth::AuthError> {
        let (status, body) = self
            .request(method, url, headers, body.to_string())
            .await
            .map_err(google_cloud_auth::AuthError::Io)?;
        Ok(google_cloud_auth::HttpResponse { status, body })
    }
}

#[async_trait]
impl aws_config::CredentialIo for BamlAuthIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env_var(key).await
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        self.file_text(path).await
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<aws_config::HttpResponse, aws_config::ConfigError> {
        let (status, body) = self
            .request(method, url, headers, String::new())
            .await
            .map_err(aws_config::ConfigError::Io)?;
        Ok(aws_config::HttpResponse { status, body })
    }

    /// Run a profile's `credential_process`. Off unless the host opts in — see
    /// the trust-boundary note in the module docs.
    async fn run_command(
        &self,
        command: &str,
    ) -> Result<aws_config::CommandOutput, aws_config::ConfigError> {
        // `credential_process` has no analogue in `RuntimeIo`; it runs as a
        // native subprocess, and is simply unavailable on wasm.
        #[cfg(not(target_arch = "wasm32"))]
        {
            // The opt-in is read through `RuntimeIo`, so the host's env policy
            // is what decides — the same gate that governs everything else
            // credential resolution can see.
            let opted_in = self
                .env_var(CREDENTIAL_PROCESS_OPT_IN)
                .await
                .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"));
            if !opted_in {
                return Err(aws_config::ConfigError::Io(format!(
                    "credential_process is disabled: it runs an arbitrary command outside BAML's \
                     IO sandbox, so it is opt-in. Set {CREDENTIAL_PROCESS_OPT_IN}=1 to allow it."
                )));
            }
            let argv = split_command(command).ok_or_else(|| {
                aws_config::ConfigError::Io(
                    "credential_process is not a runnable command line (empty, or an unterminated \
                     quote)"
                        .to_string(),
                )
            })?;
            // No shell: the argument vector goes to the OS as-is.
            let output = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .output()
                .map_err(|e| {
                    aws_config::ConfigError::Io(format!("failed to spawn credential_process: {e}"))
                })?;
            Ok(aws_config::CommandOutput {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = command;
            Err(aws_config::ConfigError::Io(
                "credential_process is not supported on wasm".into(),
            ))
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::split_command;

    #[test]
    fn splits_a_plain_command_line() {
        assert_eq!(
            split_command("aws-vault exec prod --json"),
            Some(vec![
                "aws-vault".to_string(),
                "exec".to_string(),
                "prod".to_string(),
                "--json".to_string(),
            ])
        );
    }

    #[test]
    fn honors_quotes_and_escapes() {
        assert_eq!(
            split_command(r#"/usr/bin/creds --profile "my profile" --note 'a b' --path a\ b"#),
            Some(vec![
                "/usr/bin/creds".to_string(),
                "--profile".to_string(),
                "my profile".to_string(),
                "--note".to_string(),
                "a b".to_string(),
                "--path".to_string(),
                "a b".to_string(),
            ])
        );
    }

    #[test]
    fn shell_metacharacters_stay_in_one_argument() {
        // The point of not using `sh -c`: nothing here starts a second command.
        assert_eq!(
            split_command("creds '; rm -rf /'"),
            Some(vec!["creds".to_string(), "; rm -rf /".to_string()])
        );
    }

    #[test]
    fn rejects_unrunnable_command_lines() {
        assert_eq!(split_command(""), None);
        assert_eq!(split_command("   "), None);
        assert_eq!(split_command("creds \"unterminated"), None);
    }
}
