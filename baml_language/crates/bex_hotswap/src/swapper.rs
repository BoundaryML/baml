//! The swaps a process has installed, and every check before one installs.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};

use bcs_api::hotswaps::{InstalledSwap, Swap};
use bex_engine::{BexEngine, HostCapture};
use bex_project::{
    hotswap_fingerprint::{DeclSources, declarations},
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
    built: DeclSources,
    installed: BTreeMap<String, Installed>,
    /// The last revision of each swap this process removed or retired: an
    /// older revision of it never installs again.
    removed: BTreeMap<String, u64>,
    /// Swaps retired by a replacement since [`Self::take_retired`] last ran.
    retired: Vec<InstalledSwap>,
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
            removed: BTreeMap::new(),
            retired: Vec::new(),
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

    /// Whether an installed swap replaced `id`: BCS keeps `id` live until its
    /// replacement installs everywhere, and this process must not take it back.
    pub fn is_retired(&self, id: &str) -> bool {
        self.installed
            .values()
            .any(|installed| installed.swap.replaces.as_deref() == Some(id))
    }

    /// Swaps a replacement retired, each to be reported removed.
    pub fn take_retired(&mut self) -> Vec<InstalledSwap> {
        std::mem::take(&mut self.retired)
    }

    /// The revision `id` is installed at, if it is.
    pub fn revision(&self, id: &str) -> Option<u64> {
        self.installed
            .get(id)
            .map(|installed| installed.swap.revision)
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
        // Offered again at a revision it runs: nothing installs, and nothing is recorded.
        if let Some(installed) = self.installed.get(&swap.id)
            && installed.swap.revision >= swap.revision
        {
            return Ok(installed.changes.clone());
        }
        let result = self.check_and_install(swap).await;
        match &result {
            Ok(changes) => self.audit("hotswap.installed", swap, changes, None),
            Err(error) => {
                let listed = swap.functions.iter().map(|f| f.name.clone()).collect();
                self.audit("hotswap.rejected", swap, &listed, Some(&error.0));
            }
        }
        result
    }

    async fn check_and_install(&mut self, swap: &Swap) -> Result<BTreeSet<String>, SwapperError> {
        if let Some(&removed) = self.removed.get(&swap.id)
            && removed >= swap.revision
        {
            return reject(format!(
                "revision {} of {} is not newer than revision {removed}, which this process removed",
                swap.revision, swap.id
            ));
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
            let built = self
                .build
                .sources
                .get(&function.file)
                .map_or(0, |text| text.matches(function.old_source.as_str()).count());
            match built {
                1 => {}
                0 => {
                    return reject(format!(
                        "`{}`: its source as built differs from the swap's baseline",
                        function.name
                    ));
                }
                times => {
                    return reject(format!(
                        "`{}`: its source as built appears {times} times in {}, so the swap cannot say which to replace",
                        function.name, function.file
                    ));
                }
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
        let changed_clients = changed_clients(&current_fingerprints, &next_fingerprints);
        // One swap per function: those it lists, and those it changes through
        // a client.
        let listed = swap.functions.iter().map(|function| &function.name);
        for name in listed.chain(&changes) {
            if let Some((id, _)) = self.installed.iter().find(|(id, installed)| {
                !retiring.contains(id.as_str()) && installed.changes.contains(name)
            }) {
                return reject(format!(
                    "`{name}` already has swap {id}: merge its change, then pass --replace {id}"
                ));
            }
        }
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
            if let Some(old) = self.installed.remove(retired)
                && old.swap.id != swap.id
            {
                // Retired by its replacement: the process reports it removed.
                self.removed.insert(old.swap.id.clone(), old.swap.revision);
                self.audit(
                    "hotswap.removed",
                    &old.swap,
                    &old.changes,
                    Some(&format!("replaced by {}", swap.id)),
                );
                self.retired.push(InstalledSwap {
                    id: old.swap.id,
                    revision: old.swap.revision,
                });
            }
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

    /// Whether the marks on the changed function `name` let this change in:
    /// every declaration under its key that changed must be marked, keep its
    /// mark, and its rule must accept.
    fn check_rule(
        &self,
        name: &str,
        next: &DeclSources,
        fingerprints: &bex_vm_types::DeclFingerprints,
        changed_clients: &BTreeSet<String>,
    ) -> Result<(), String> {
        let key = self
            .built
            .keys()
            .find(|key| is_function_key(key) && label(key) == name)
            .ok_or_else(|| format!("`{name}` is new, and a swap cannot add declarations"))?;
        let originals = &self.built[key];
        let replacements = next
            .get(key)
            .filter(|replacements| replacements.len() == originals.len())
            .ok_or_else(|| format!("`{name}` was added or deleted, which needs a deploy"))?;
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
        // A key implementations share pairs up in source order, which a swap,
        // replacing text in place, keeps.
        for (original, replacement) in originals.iter().zip(replacements) {
            if original.text == replacement.text && uses.is_empty() {
                continue;
            }
            self.check_declaration(name, &original.text, &replacement.text, &uses)?;
        }
        Ok(())
    }

    fn check_declaration(
        &self,
        name: &str,
        original: &str,
        replacement: &str,
        uses: &[String],
    ) -> Result<(), String> {
        let mark = match hotswap_rules::mark_of(original) {
            Some(Ok(mark)) => mark,
            Some(Err(problem)) => return Err(format!("{name}: {problem}")),
            None => {
                return Err(format!(
                    "`{name}` is not marked `/// baml:hotswap`, so changing it needs a deploy"
                ));
            }
        };
        if hotswap_rules::mark_of(replacement) != Some(Ok(mark.clone())) {
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
        let shipped = |function: &str| {
            self.built
                .iter()
                .find(|(key, sources)| {
                    is_function_key(key) && label(key) == function && sources.len() == 1
                })
                .map(|(_, sources)| sources[0].text.clone())
        };
        hotswap_rules::check(
            &rule,
            &Change {
                original,
                replacement,
                changed_clients: uses,
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
        // A failed removal leaves the swap running, so only a removal is recorded.
        if result.is_ok()
            && let Some(old) = self.installed.remove(id)
        {
            self.removed.insert(old.swap.id.clone(), old.swap.revision);
            self.audit("hotswap.removed", &removed, &old.changes, Some(reason));
        }
        result
    }

    /// Record an audit event in the process's own recording: a span named
    /// `event`, with the swap's ID, publisher, approver, the functions it
    /// changed and the reason.
    fn audit(&self, event: &str, swap: &Swap, functions: &BTreeSet<String>, reason: Option<&str>) {
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
                    functions
                        .iter()
                        .map(|name| HostCapture::String(name.clone()))
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

/// Clients whose configuration differs between `current` and `next`, and
/// every client built from one of them, by simple name: a client built from a
/// changed client holds the changed one too.
fn changed_clients(
    current: &bex_vm_types::DeclFingerprints,
    next: &bex_vm_types::DeclFingerprints,
) -> BTreeSet<String> {
    let simple = |key: &str| {
        label(key)
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let mut changed: BTreeSet<String> = next
        .decls
        .iter()
        .filter(|(key, text)| key.starts_with("client ") && current.decls.get(*key) != Some(*text))
        .map(|(key, _)| simple(key))
        .collect();
    loop {
        let reached: Vec<String> = next
            .decls
            .keys()
            .filter(|key| key.starts_with("client ") && !changed.contains(&simple(key)))
            .filter(|key| {
                next.refs
                    .get(*key)
                    .is_some_and(|refs| refs.iter().any(|name| changed.contains(name)))
            })
            .map(|key| simple(key))
            .collect();
        if reached.is_empty() {
            return changed;
        }
        changed.extend(reached);
    }
}

/// The first seven characters of a commit, whatever BCS sent.
fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    use bex_vm_types::DeclFingerprints;

    use super::changed_clients;

    fn fingerprints(decls: &[(&str, &str)]) -> DeclFingerprints {
        DeclFingerprints {
            decls: decls
                .iter()
                .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
                .collect(),
            refs: decls
                .iter()
                .map(|(key, text)| {
                    let words = text.split_whitespace().map(str::to_string).collect();
                    ((*key).to_string(), words)
                })
                .collect(),
        }
    }

    /// A client built from a changed client holds the change too, so rules
    /// see it for every function that uses either.
    #[test]
    fn a_client_built_from_a_changed_client_changes_too() {
        let before = fingerprints(&[
            ("client Base", "make 1"),
            ("client Wrapped", "retry Base"),
            ("client Other", "make 2"),
        ]);
        let after = fingerprints(&[
            ("client Base", "make 3"),
            ("client Wrapped", "retry Base"),
            ("client Other", "make 2"),
        ]);
        assert_eq!(
            changed_clients(&before, &after)
                .into_iter()
                .collect::<Vec<_>>(),
            ["Base", "Wrapped"]
        );
    }
}
