//! Function hot swapping from Boundary Cloud in a running process (BEP-087,
//! stage 2).
//!
//! A process registers with BCS, receives the swaps that match it, and checks
//! each one before it installs: its build must come from the swap's
//! baseline commit, every function it changes must be marked
//! `/// baml:hotswap`, each mark's rule must accept the change, and at most
//! one swap may change a function. [`Swapper`] does that and installs through
//! the engine; [`agent`] talks to BCS.
//!
//! A process swaps by splicing each installed swap's new function text into
//! its own source as built, and recompiling it against the running program
//! (`BexEngine::hotswap_patch_named`), which rejects any change to the public
//! interface. Rules see exactly the text that compiles.

pub mod agent;
mod swapper;

use std::{collections::BTreeMap, sync::Arc};

pub use baml_db::manifest::HotswapConfig;
pub use swapper::{LAPSE, Swapper, SwapperError};

/// What a process knows of its own build.
#[derive(Debug, Clone)]
pub struct HotswapBuild {
    /// The project's source as built, by project-relative path.
    pub sources: BTreeMap<String, String>,
    /// The git commit the build came from, when it records one.
    pub commit: Option<String>,
    /// Whether the package had changes since `commit` when built.
    pub dirty: bool,
    pub config: HotswapConfig,
}

impl HotswapBuild {
    /// The functions marked `/// baml:hotswap`, by the names swaps report,
    /// for `EngineConfig::hotswap_marked`. An empty set means the project
    /// has nothing to swap.
    pub fn marked(&self) -> std::collections::BTreeSet<String> {
        let (_, declarations) = bex_project::hotswap_fingerprint::declarations(
            self.sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        );
        // A key several declarations share (implementations are keyed by bare
        // names) is swappable only when every one of them is marked.
        declarations
            .iter()
            .filter(|(key, _)| is_function_key(key))
            .filter(|(_, sources)| {
                sources.iter().all(|source| {
                    matches!(
                        bex_project::hotswap_rules::mark_of(&source.text),
                        Some(Ok(_))
                    )
                })
            })
            .map(|(key, _)| label(key).to_string())
            .collect()
    }

    /// Why the build's marks cannot run: a mark that names no rule while
    /// `[hotswap]` sets no `default_rule`.
    pub fn check_marks(&self) -> Result<(), String> {
        if self.config.default_rule.is_some() {
            return Ok(());
        }
        let (_, declarations) = bex_project::hotswap_fingerprint::declarations(
            self.sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        );
        let unruled: Vec<String> = declarations
            .iter()
            .filter(|(key, _)| is_function_key(key))
            .filter(|(_, sources)| {
                sources.iter().any(|source| {
                    matches!(
                        bex_project::hotswap_rules::mark_of(&source.text),
                        Some(Ok(bex_project::hotswap_rules::Mark { rule: None }))
                    )
                })
            })
            .map(|(key, _)| format!("`{}`", label(key)))
            .collect();
        if unruled.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} {} marked `/// baml:hotswap` with no rule, and baml.toml sets no `[hotswap] default_rule`",
                unruled.join(", "),
                if unruled.len() == 1 { "is" } else { "are" }
            ))
        }
    }

    /// The fingerprints the engine compares every swap with.
    pub fn baseline(&self) -> bex_vm_types::DeclFingerprints {
        bex_project::hotswap_fingerprint::declaration_fingerprints(
            self.sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        )
    }
}

/// Whether `commit` is `baseline`, either possibly abbreviated (at least 7
/// hex digits).
pub fn same_commit(commit: &str, baseline: &str) -> bool {
    let (short, long) = if commit.len() <= baseline.len() {
        (commit, baseline)
    } else {
        (baseline, commit)
    };
    short.len() >= 7 && long.starts_with(short)
}

pub(crate) fn is_function_key(key: &str) -> bool {
    key.starts_with("fn ") || key.starts_with("method ") || key.starts_with("implmethod ")
}

pub(crate) fn label(key: &str) -> &str {
    key.split_once(' ').map_or(key, |(_, name)| name)
}

/// Everything a host needs to make a process swappable from Boundary Cloud,
/// decided before the engine is built.
pub struct Prepared {
    pub build: Arc<HotswapBuild>,
    pub link: agent::CloudLink,
}

/// Why hot swapping stays off for a process, when nothing is wrong.
fn off(why: &str) -> Option<Prepared> {
    tracing::debug!("hot swapping is off: {why}");
    None
}

impl Prepared {
    /// Whether a process built from `build`, recording to `recording`, can be
    /// swapped: hot swapping is on in `baml.toml` and not turned off with
    /// `disabled` (`--no-hotswap` or the host option), the project marks a
    /// function, and recordings go to Boundary Cloud. A mark without a rule
    /// and no `default_rule` is an error.
    pub fn new(
        build: HotswapBuild,
        disabled: bool,
        recording: &bex_engine::TelemetryRecording,
    ) -> Result<Option<Self>, String> {
        if disabled || !build.config.enabled {
            return Ok(off("turned off"));
        }
        if build.marked().is_empty() {
            return Ok(off("no function is marked `/// baml:hotswap`"));
        }
        build.check_marks()?;
        let Some(authorization) = recording.cloud_link() else {
            return Ok(off("recordings stay local"));
        };
        let endpoint =
            bcs_api::Endpoint::with_default(Some(authorization.authentication.endpoint()))
                .map_err(|error| error.to_string())?;
        let client = bcs_api::Client::new(endpoint).map_err(|error| error.to_string())?;
        Ok(Some(Self {
            build: Arc::new(build),
            link: agent::CloudLink {
                client,
                authorization,
            },
        }))
    }

    /// The engine options that make it swappable.
    pub fn configure(&self, config: &mut bex_engine::EngineConfig) {
        config.hotswap_baseline = Some(self.build.baseline());
        config.hotswap_marked = Some(self.build.marked());
    }

    /// Start the agent for `engine`, built with [`Self::configure`], and wait
    /// up to `startup_wait` for its swaps.
    pub fn start(self, engine: Arc<bex_engine::BexEngine>, host: &str) -> Option<agent::Agent> {
        let wait = self.build.config.startup_wait;
        let process_id: String = bex_engine::process_id_hex();
        let agent = agent::start(
            engine,
            self.build,
            self.link,
            agent::AgentOptions {
                process_id,
                baml_version: env!("CARGO_PKG_VERSION").to_string(),
                host: host.to_string(),
                lapse: None,
                retry: None,
            },
        );
        match agent {
            Ok(agent) => {
                agent.wait_ready(wait);
                Some(agent)
            }
            Err(error) => {
                tracing::warn!(%error, "hot swapping is off: its thread did not start");
                None
            }
        }
    }
}

/// The functions a source change swaps, as the engine names them.
pub use bex_vm::hotswap::hotswap_changes;
