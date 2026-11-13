//! The swaps a process has installed, and every check before one installs.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};

use bcs_api::hotswaps::{InstalledSwap, Swap};
use bex_engine::{BexEngine, HostCapture};
use bex_project::{
    hotswap_fingerprint::{DeclSource, declarations},
    hotswap_rules::{self, Change, Rule},
};
use indexmap::IndexMap;

use crate::{HotswapBuild, is_function_key, label, same_commit};

/// How long a swap stays installed without a renewal from BCS.
pub const LAPSE: Duration = Duration::from_secs(3600);

/// Why a swap did not install. The message reaches its publisher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapperError(pub String);

impl std::fmt::Display for SwapperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn reject<T>(message: impl Into<String>) -> Result<T, SwapperError> {
    Err(SwapperError(message.into()))
}

struct Installed {
    swap: Swap,
    /// Every function the swap changes, its functions and any that use a
    /// client it changes, by the names the engine reports.
    changes: BTreeSet<String>,
    renewed: Instant,
}

/// The swaps one engine runs, by ID, with when BCS last renewed each.
pub struct Swapper {
    engine: Arc<BexEngine>,
    build: Arc<HotswapBuild>,
    /// Each declaration as built, by fingerprint key.
    built: BTreeMap<String, DeclSource>,
    installed: BTreeMap<String, Installed>,
    lapse: Duration,
}

impl Swapper {
    pub fn new(engine: Arc<BexEngine>, build: Arc<HotswapBuild>) -> Self {
        let (_, built) = declarations(
            build
                .sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        );
        Self {
            engine,
            build,
            built,
            installed: BTreeMap::new(),
            lapse: LAPSE,
        }
    }

    /// Lapse swaps after `lapse` instead of an hour, for tests.
    #[must_use]
    pub fn with_lapse(mut self, lapse: Duration) -> Self {
        self.lapse = lapse;
        self
    }

    pub fn build(&self) -> &HotswapBuild {
        &self.build
    }

    /// The swaps installed, as every request to BCS lists them.
    pub fn installed(&self) -> Vec<InstalledSwap> {
        self.installed
            .values()
            .map(|installed| InstalledSwap {
                id: installed.swap.id.clone(),
                revision: installed.swap.revision,
            })
            .collect()
    }

    pub fn is_installed(&self, id: &str, revision: u64) -> bool {
        self.installed
            .get(id)
            .is_some_and(|installed| installed.swap.revision >= revision)
    }

    /// BCS vouched for `id` again.
    pub fn renew(&mut self, id: &str) {
        if let Some(installed) = self.installed.get_mut(id) {
            installed.renewed = Instant::now();
        }
    }

    /// The swaps BCS has not renewed within the lapse.
    pub fn lapsed(&self) -> Vec<String> {
        self.installed
            .iter()
            .filter(|(_, installed)| installed.renewed.elapsed() >= self.lapse)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The project's source with each of `swaps` spliced in.
    fn spliced<'a>(&self, swaps: impl Iterator<Item = &'a Swap>) -> IndexMap<String, String> {
        let mut files: IndexMap<String, String> = self
            .build
            .sources
            .iter()
            .map(|(path, text)| (path.clone(), text.clone()))
            .collect();
        for swap in swaps {
            for function in &swap.functions {
                if let Some(text) = files.get_mut(&function.file) {
                    *text = text.replacen(&function.old_source, &function.new_source, 1);
                }
            }
        }
        files
    }

    /// Check `swap` and install it at the next park. Returns the functions
    /// it changes. A swap that `replaces` another retires it in the same
    /// install; a newer revision of an installed swap replaces it too.
    pub async fn install(&mut self, swap: &Swap) -> Result<BTreeSet<String>, SwapperError> {
        let result = self.check_and_install(swap).await;
        let (event, reason) = match &result {
            Ok(_) => ("hotswap.installed", None),
            Err(error) => ("hotswap.rejected", Some(error.0.as_str())),
        };
        self.audit(event, swap, reason);
        result
    }

    async fn check_and_install(&mut self, swap: &Swap) -> Result<BTreeSet<String>, SwapperError> {
        if let Some(installed) = self.installed.get(&swap.id)
            && installed.swap.revision >= swap.revision
        {
            return Ok(installed.changes.clone());
        }
        match &self.build.commit {
            Some(commit) if same_commit(commit, &swap.baseline) => {}
            Some(commit) => {
                return reject(format!(
                    "the process was built from {}, not the baseline {}",
                    short(commit),
                    short(&swap.baseline)
                ));
            }
            None => return reject("the process's build records no git commit"),
        }
        if self.build.config.require_approval && swap.approved_by.is_none() {
            return reject("the swap was not approved, and this build requires approval");
        }
        let retiring: BTreeSet<&str> = std::iter::once(swap.id.as_str())
            .chain(swap.replaces.as_deref())
            .collect();
        for function in &swap.functions {
            if let Some((id, _)) = self.installed.iter().find(|(id, installed)| {
                !retiring.contains(id.as_str())
                    && installed
                        .swap
                        .functions
                        .iter()
                        .any(|other| other.name == function.name)
            }) {
                return reject(format!(
                    "`{}` already has swap {id}: merge its change, then pass --replace {id}",
                    function.name
                ));
            }
            let built = self
                .build
                .sources
                .get(&function.file)
                .map_or(0, |text| text.matches(function.old_source.as_str()).count());
            if built != 1 {
                return reject(format!(
                    "`{}`: its source as built differs from the swap's baseline",
                    function.name
                ));
            }
        }
        let keep: Vec<&Swap> = self
            .installed
            .values()
            .filter(|installed| !retiring.contains(installed.swap.id.as_str()))
            .map(|installed| &installed.swap)
            .collect();
        let current = self.spliced(keep.iter().copied());
        let next = self.spliced(keep.iter().copied().chain(std::iter::once(swap)));
        let (current_fingerprints, _) =
            declarations(current.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        let (next_fingerprints, next_sources) =
            declarations(next.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        let changes = bex_vm::hotswap::hotswap_changes(&current_fingerprints, &next_fingerprints);
        let changed_clients: BTreeSet<String> = next_fingerprints
            .decls
            .iter()
            .filter(|(key, text)| {
                key.starts_with("client ") && current_fingerprints.decls.get(*key) != Some(*text)
            })
            .map(|(key, _)| {
                label(key)
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        let mut problems = Vec::new();
        for name in &changes {
            if let Err(problem) =
                self.check_rule(name, &next_sources, &next_fingerprints, &changed_clients)
            {
                problems.push(problem);
            }
        }
        if !problems.is_empty() {
            return reject(problems.join("\n"));
        }
        let mut swap_ids: BTreeMap<String, Arc<str>> = BTreeMap::new();
        for installed in self.installed.values() {
            if retiring.contains(installed.swap.id.as_str()) {
                continue;
            }
            let id: Arc<str> = installed.swap.id.as_str().into();
            for name in &installed.changes {
                swap_ids.insert(name.clone(), id.clone());
            }
        }
        let id: Arc<str> = swap.id.as_str().into();
        for name in &changes {
            swap_ids.insert(name.clone(), id.clone());
        }
        self.engine
            .hotswap_patch_named(next, &swap_ids)
            .await
            .map_err(|error| SwapperError(error.to_string()))?;
        for retired in retiring {
            self.installed.remove(retired);
        }
        self.installed.insert(
            swap.id.clone(),
            Installed {
                swap: swap.clone(),
                changes: changes.clone(),
                renewed: Instant::now(),
            },
        );
        Ok(changes)
    }

    /// Whether the mark on the changed function `name` lets this change in:
    /// it must be marked, keep its mark, and its rule must accept.
    fn check_rule(
        &self,
        name: &str,
        next: &BTreeMap<String, DeclSource>,
        fingerprints: &bex_vm_types::DeclFingerprints,
        changed_clients: &BTreeSet<String>,
    ) -> Result<(), String> {
        let key = self
            .built
            .keys()
            .find(|key| is_function_key(key) && label(key) == name)
            .ok_or_else(|| format!("`{name}` is new, and a swap cannot add declarations"))?;
        let original = &self.built[key];
        let replacement = next
            .get(key)
            .ok_or_else(|| format!("`{name}` was deleted, which needs a deploy"))?;
        let mark = match hotswap_rules::mark_of(&original.text) {
            Some(Ok(mark)) => mark,
            Some(Err(problem)) => return Err(format!("{name}: {problem}")),
            None => {
                return Err(format!(
                    "`{name}` is not marked `/// baml:hotswap`, so changing it needs a deploy"
                ));
            }
        };
        if hotswap_rules::mark_of(&replacement.text) != Some(Ok(mark.clone())) {
            return Err(format!(
                "`{name}`: a swap cannot change its `/// baml:hotswap` mark, which needs a deploy"
            ));
        }
        let rule: Rule = mark
            .rule
            .or_else(|| self.build.config.default_rule.clone())
            .ok_or_else(|| {
                format!("`{name}` names no rule, and baml.toml sets no `[hotswap] default_rule`")
            })?;
        let uses: Vec<String> = fingerprints
            .refs
            .get(key)
            .map(|refs| {
                refs.iter()
                    .filter(|name| changed_clients.contains(*name))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let shipped = |function: &str| {
            self.built
                .iter()
                .find(|(key, _)| is_function_key(key) && label(key) == function)
                .map(|(_, source)| source.text.clone())
        };
        hotswap_rules::check(
            &rule,
            &Change {
                original: &original.text,
                replacement: &replacement.text,
                changed_clients: &uses,
            },
            &shipped,
        )
        .map_err(|problem| format!("{name}: {}: {problem}", rule.name()))
    }

    /// Remove swap `id`, returning its functions to the code as built, or
    /// to no other swap's code. `reason` is `removed`, `disabled`,
    /// `deleted` or `lapsed`, for the audit event.
    pub async fn remove(&mut self, id: &str, reason: &str) -> Result<(), SwapperError> {
        let Some(removed) = self
            .installed
            .get(id)
            .map(|installed| installed.swap.clone())
        else {
            return Ok(());
        };
        let keep: Vec<&Swap> = self
            .installed
            .values()
            .filter(|installed| installed.swap.id != id)
            .map(|installed| &installed.swap)
            .collect();
        let files = self.spliced(keep.iter().copied());
        let mut swap_ids: BTreeMap<String, Arc<str>> = BTreeMap::new();
        for installed in self.installed.values().filter(|i| i.swap.id != id) {
            let swap_id: Arc<str> = installed.swap.id.as_str().into();
            for name in &installed.changes {
                swap_ids.insert(name.clone(), swap_id.clone());
            }
        }
        let result = self
            .engine
            .hotswap_patch_named(files, &swap_ids)
            .await
            .map(|_| ())
            .map_err(|error| SwapperError(error.to_string()));
        if result.is_ok() {
            self.installed.remove(id);
        }
        self.audit("hotswap.removed", &removed, Some(reason));
        result
    }

    /// Record an audit event in the process's own recording: a span named
    /// `event`, with the swap's ID, publisher, approver and the reason.
    fn audit(&self, event: &str, swap: &Swap, reason: Option<&str>) {
        let definition = bex_engine::HostDefinition {
            language: "baml".into(),
            module: "hotswap".into(),
            qualified_name: event.into(),
            source_file: String::new(),
            definition_line: 0,
            wrapper_line: 0,
            display_name: event.into(),
        };
        let text =
            |value: &Option<String>| value.clone().map_or(HostCapture::Null, HostCapture::String);
        let inputs = HostCapture::Map(vec![
            ("hotswap_id".into(), HostCapture::String(swap.id.clone())),
            (
                "revision".into(),
                HostCapture::Int(i64::try_from(swap.revision).unwrap_or(i64::MAX)),
            ),
            (
                "baseline".into(),
                HostCapture::String(swap.baseline.clone()),
            ),
            ("published_by".into(), text(&swap.published_by)),
            ("approved_by".into(), text(&swap.approved_by)),
            (
                "functions".into(),
                HostCapture::List(
                    swap.functions
                        .iter()
                        .map(|function| HostCapture::String(function.name.clone()))
                        .collect(),
                ),
            ),
            (
                "reason".into(),
                reason.map_or(HostCapture::Null, |reason| {
                    HostCapture::String(reason.into())
                }),
            ),
        ]);
        match self.engine.begin_host_invocation(
            &definition,
            None,
            &bex_vm_types::trace::TraceOptionsData {
                inputs: Some(true),
                ..Default::default()
            },
            &bex_engine::HostCallSite {
                source_file: String::new(),
                line: 0,
            },
            Some(&inputs),
        ) {
            Ok(invocation) => invocation.finish(btel_types::InvocationOutcome::Ok),
            Err(error) => tracing::warn!(%error, "hot swap audit event not recorded"),
        }
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}
