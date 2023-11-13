use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Diagnostics, SourceFile, Span};
use serde::{Deserialize, Serialize};

use crate::configuration::Generator;

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    cli_version: Option<semver::Version>,
    client_version: Option<semver::Version>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFileWrapper {
    version: u32,
    content: LockFile,

    #[serde(skip)]
    span: Option<Span>,
}

impl LockFileWrapper {
    pub fn cli_version(&self) -> Option<&semver::Version> {
        self.content.cli_version.as_ref()
    }

    pub fn client_version(&self) -> Option<&semver::Version> {
        self.content.client_version.as_ref()
    }

    pub fn from_generator(gen: &Generator) -> Result<Self, String> {
        Ok(Self {
            version: 1,
            content: LockFile {
                cli_version: Some(
                    semver::Version::parse(env!("CARGO_PKG_VERSION"))
                        .map_err(|e| format!("{} {}", env!("CARGO_PKG_VERSION"), e.to_string()))?,
                ),
                client_version: gen.client_version().and_then(|f| {
                    let res = semver::Version::parse(f).map_err(|e| {
                        format!(
                            "{} {}",
                            gen.client_version().unwrap_or("<unknown>"),
                            e.to_string()
                        )
                    });
                    if res.is_err() {
                        log::warn!(
                            "Failed to parse client version: {}",
                            gen.client_version().unwrap_or("<unknown>")
                        );
                    }
                    res.ok()
                }),
            },
            span: gen.span.clone(),
        })
    }

    pub fn from_path(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let content = std::fs::read_to_string(&path_buf)?;
        let mut parsed: LockFileWrapper = serde_json::from_str(&content)?;
        let len = content.len();
        parsed.span = Some(Span::new(SourceFile::from((path_buf, content)), 0, len));
        Ok(parsed)
    }

    pub fn validate(&self, prev: &LockFileWrapper, diag: &mut Diagnostics) {
        assert!(self.span.is_some());
        assert!(prev.span.is_some());

        match (&self.content.cli_version, &prev.content.cli_version) {
            (Some(_), None) => {
                // Ok as prev is not set, but current exists.
            }
            (None, Some(b)) => {
                // Not ok as prev is set, but current is not.
                diag.push_error(DatamodelError::new_validation_error(
                    &format!(
                        "The last CLI version was {}. The current version of baml isn't found.",
                        b
                    ),
                    self.span.clone().unwrap(),
                ));
            }
            (Some(a), Some(b)) => {
                match a.cmp(b) {
                    std::cmp::Ordering::Less => {
                        // Not ok as prev is newer than current.
                        diag.push_error(DatamodelError::new_validation_error(
                            &format!("The last CLI version was {}. You're currently at: {}. Please run `baml update`", b,a),
                            self.span.clone().unwrap(),
                        ));
                    }
                    std::cmp::Ordering::Equal => {}
                    std::cmp::Ordering::Greater => {
                        // Ok as prev is older than current.
                        diag.push_warning(DatamodelWarning::new(
                            format!("Upgrading generated code with latest CLI: {}", a),
                            self.span.clone().unwrap(),
                        ));
                    }
                }
            }
            (None, None) => {}
        }

        match (&self.content.client_version, &prev.content.client_version) {
            (Some(_), None) => {
                // Ok as prev is not set, but current exists.
            }
            (None, Some(b)) => {
                // Not ok as prev is set, but current is not.
                diag.push_warning(DatamodelWarning::new(
                    format!(
                        "The last client version was {}. The current version of the client isn't found. Have you run `baml update-client`?",
                        b
                    ),
                    self.span.clone().unwrap(),
                ));
            }
            (Some(a), Some(b)) => {
                match a.cmp(b) {
                    std::cmp::Ordering::Less => {
                        // Not ok as prev is newer than current.
                        diag.push_error(DatamodelError::new_validation_error(
                            &format!("The last client version was {}. You're using an older version: {}. Please run: `baml update-client`", b,a),
                            self.span.clone().unwrap(),
                        ));
                    }
                    std::cmp::Ordering::Equal => {}
                    std::cmp::Ordering::Greater => {
                        // Ok as prev is older than current.
                        diag.push_warning(DatamodelWarning::new(
                            format!("Upgrading generated code with latest baml client: {}", a),
                            self.span.clone().unwrap(),
                        ));
                    }
                }
            }
            (None, None) => {}
        }
    }
}
