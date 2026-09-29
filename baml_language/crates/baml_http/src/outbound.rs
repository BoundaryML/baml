//! Every destination BAML contacts on its own.
//!
//! BAML's own requests (telemetry, login, self-update, the remote cache) must
//! name a [`Destination`]; there is no way to make one without. That makes
//! this file the complete list, and the table in `VENDOR.md` (generated from
//! it, see [`audit_table`]) the audit document.
//!
//! Requests a BAML program makes (LLM calls, `baml.http`, `baml.ws`) are not
//! listed: the program chose where they go.
//!
//! In a `no-phone-home` build every destination except the user's own remote
//! cache always fails, before reaching the transport, and the built-in URLs
//! are not compiled in.

use crate::{Bytes, Error, Headers, Request, Response};

/// A place BAML contacts on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Destination {
    /// Anonymous CLI usage telemetry.
    AnonymousTelemetry,
    /// `baml feedback`.
    Feedback,
    /// Cloud recording upload and its heartbeat.
    CloudTelemetry,
    /// `baml auth login`.
    Auth,
    /// The release manifest (self-update, `baml` toolchain installs).
    ReleaseManifest,
    /// Release archive downloads.
    ReleaseDownload,
    /// The remote bytecode cache, at the URL the user sets.
    RemoteCache,
}

/// What [`audit_table`] reports for a destination.
struct Entry {
    name: &'static str,
    /// Where requests go by default; `None` when the user configures it.
    default_url: Option<&'static str>,
    when: &'static str,
    purpose: &'static str,
}

// The built-in URLs. Not compiled into a `no-phone-home` build.
#[cfg(not(feature = "no-phone-home"))]
const POSTHOG: Option<&str> = Some("https://us.i.posthog.com");
#[cfg(not(feature = "no-phone-home"))]
const WORKOS: Option<&str> = Some("https://api.workos.com");
#[cfg(not(feature = "no-phone-home"))]
const RELEASE_MANIFEST: Option<&str> = Some("https://pkg.boundaryml.com/manifest/v1");
#[cfg(not(feature = "no-phone-home"))]
const RELEASE_DOWNLOAD: Option<&str> = Some("https://github.com");
#[cfg(feature = "no-phone-home")]
const POSTHOG: Option<&str> = None;
#[cfg(feature = "no-phone-home")]
const WORKOS: Option<&str> = None;
#[cfg(feature = "no-phone-home")]
const RELEASE_MANIFEST: Option<&str> = None;
#[cfg(feature = "no-phone-home")]
const RELEASE_DOWNLOAD: Option<&str> = None;

impl Destination {
    pub const ALL: [Destination; 7] = [
        Destination::AnonymousTelemetry,
        Destination::Feedback,
        Destination::CloudTelemetry,
        Destination::Auth,
        Destination::ReleaseManifest,
        Destination::ReleaseDownload,
        Destination::RemoteCache,
    ];

    fn entry(self) -> Entry {
        match self {
            Destination::AnonymousTelemetry => Entry {
                name: "AnonymousTelemetry",
                default_url: POSTHOG,
                when: "Each CLI run, unless telemetry is turned off",
                purpose: "Anonymous CLI usage events",
            },
            Destination::Feedback => Entry {
                name: "Feedback",
                default_url: POSTHOG,
                when: "`baml feedback`",
                purpose: "The feedback the user submits",
            },
            Destination::CloudTelemetry => Entry {
                name: "CloudTelemetry",
                default_url: None,
                when: "Only when cloud recording is configured",
                purpose: "Uploads recordings and sends a heartbeat",
            },
            Destination::Auth => Entry {
                name: "Auth",
                default_url: WORKOS,
                when: "`baml auth login`",
                purpose: "Device-code login",
            },
            Destination::ReleaseManifest => Entry {
                name: "ReleaseManifest",
                default_url: RELEASE_MANIFEST,
                when: "Self-update and `baml` toolchain installs",
                purpose: "Finds the available releases",
            },
            Destination::ReleaseDownload => Entry {
                name: "ReleaseDownload",
                default_url: RELEASE_DOWNLOAD,
                when: "Self-update and `baml` toolchain installs",
                purpose: "Downloads release archives and checksums",
            },
            Destination::RemoteCache => Entry {
                name: "RemoteCache",
                default_url: None,
                when: "Only when `BAML_CACHE_REMOTE` is set",
                purpose: "Shares compiled bytecode through the user's own server",
            },
        }
    }

    pub fn name(self) -> &'static str {
        self.entry().name
    }

    /// The built-in URL, when there is one and this build may use it.
    pub fn default_url(self) -> Option<&'static str> {
        self.entry().default_url
    }

    /// Whether this contacts anything other than a server the user runs.
    pub fn phones_home(self) -> bool {
        !matches!(self, Destination::RemoteCache)
    }

    /// Whether this build may contact it at all.
    pub fn enabled(self) -> bool {
        !(cfg!(feature = "no-phone-home") && self.phones_home())
    }

    /// `Ok` when this build may contact the destination.
    pub fn check(self) -> Result<(), Error> {
        if self.enabled() {
            Ok(())
        } else {
            Err(Error::unsupported(format!(
                "{} is disabled in this build (no-phone-home)",
                self.name()
            )))
        }
    }
}

/// Send one of BAML's own requests.
pub async fn send(destination: Destination, request: Request) -> Result<Response, Error> {
    destination.check()?;
    crate::provider().send(request).await
}

/// A response read to the end, from [`send_blocking`].
#[derive(Debug, Clone)]
pub struct BlockingResponse {
    pub status: u16,
    pub headers: Headers,
    pub url: String,
    pub body: Bytes,
}

impl BlockingResponse {
    /// The first value of header `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// [`send`] for synchronous code, reading the body into memory. A body longer
/// than `max_body` bytes is an error, so a hostile or broken server cannot
/// make it allocate without bound. Runs on its own thread, so it is safe to
/// call from inside an async runtime.
pub fn send_blocking(
    destination: Destination,
    request: Request,
    max_body: usize,
) -> Result<BlockingResponse, Error> {
    destination.check()?;
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::io(format!("could not start an HTTP runtime: {e}")))?;
        runtime.block_on(async move {
            let response = crate::provider().send(request).await?;
            let (status, headers, url) = (
                response.status,
                response.headers.clone(),
                response.url.clone(),
            );
            let body = crate::collect(response.body, max_body)
                .await
                .map_err(|e| match e {
                    crate::CollectError::Body(e) => e,
                    crate::CollectError::TooLarge => {
                        Error::io(format!("response body exceeds {max_body} bytes"))
                    }
                })?;
            Ok(BlockingResponse {
                status,
                headers,
                url,
                body,
            })
        })
    })
    .join()
    .unwrap_or_else(|_| Err(Error::io("the HTTP request thread panicked")))
}

/// The outbound-destinations table in `VENDOR.md`, as this build sees it.
pub fn audit_table() -> String {
    let mut out = String::from(
        "| Destination | Default URL | When | Purpose |\n\
         |---|---|---|---|\n",
    );
    for destination in Destination::ALL {
        use std::fmt::Write as _;
        let entry = destination.entry();
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            entry.name,
            entry.default_url.map_or_else(
                || "configured by the user".to_string(),
                |u| format!("`{u}`")
            ),
            entry.when,
            entry.purpose,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "no-phone-home"))]
    #[test]
    fn vendor_md_lists_every_destination() {
        const BEGIN: &str = "<!-- outbound:begin -->\n";
        const END: &str = "<!-- outbound:end -->";
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../VENDOR.md");
        let doc = std::fs::read_to_string(path).unwrap();
        let start = doc
            .find(BEGIN)
            .expect("VENDOR.md has an outbound:begin marker")
            + BEGIN.len();
        let end = doc.find(END).expect("VENDOR.md has an outbound:end marker");
        let expected = audit_table();
        if std::env::var_os("BAML_UPDATE_OUTBOUND").is_some() {
            std::fs::write(path, format!("{}{expected}{}", &doc[..start], &doc[end..])).unwrap();
            return;
        }
        assert_eq!(
            &doc[start..end],
            expected,
            "VENDOR.md's outbound table is stale; rerun with BAML_UPDATE_OUTBOUND=1"
        );
    }

    #[cfg(feature = "no-phone-home")]
    #[test]
    fn no_phone_home_refuses_everything_but_the_remote_cache() {
        for destination in Destination::ALL {
            let refused = destination.check().is_err();
            assert_eq!(
                refused,
                destination != Destination::RemoteCache,
                "{destination:?}"
            );
            assert_eq!(destination.default_url(), None, "{destination:?}");
        }
    }

    #[cfg(not(feature = "no-phone-home"))]
    #[test]
    fn default_build_allows_everything() {
        for destination in Destination::ALL {
            assert!(destination.check().is_ok(), "{destination:?}");
        }
    }
}
