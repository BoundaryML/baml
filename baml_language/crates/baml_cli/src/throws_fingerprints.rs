//! Persisted dependency fingerprints for the throws cache (Stage 1, shadow).
//!
//! The warm path serves each clean function's solved transitive throws from
//! the manifest. Today the only defense against serving a stale value is the
//! inference-priced gate (`reuse_throws_mismatches`), which re-derives every
//! clean function's throws honestly — the dominant cost of a warm dirty
//! check. This module makes validity checkable *by construction* instead:
//! at store time, each file gets a fingerprint folding the identity of every
//! input its functions' solved throws depend on; at plan time the same
//! computation runs over the *current* inputs, and a stored value is valid
//! iff the fingerprints match. Pure hashing — no inference.
//!
//! The dependency graph comes from the syntactic throw-fact layer
//! ([`FunctionThrowFacts`]): per function, its direct throw facts, its
//! same-package call edges, and whether a closed `throws` clause firewalls
//! propagation. The graph is condensed into SCCs (mutual recursion shares
//! one joint fingerprint) and folded bottom-up in dependency order.
//!
//! Inputs folded per node, and why each is sufficient for what it covers:
//!
//! - **Own facts** (`borsh(FunctionThrowFacts)`): the function's declared or
//!   syntactic throw sites and its call-edge list. A body edit that changes
//!   what the function throws or calls changes this term. (Facts are a pure
//!   function of file content, so a clean file's stored facts ARE its
//!   current facts.)
//! - **Callee fingerprints** (resolved same-package edges): transitive
//!   throws flow; a change anywhere in the callee cone changes this term.
//! - **The environment term**, folded into every node that is not firewalled
//!   behind a closed `throws` clause and has at least one *unresolved* call
//!   edge (a dotted path that names no known function node — method calls
//!   on values, cross-package calls, function-typed locals): the coarse,
//!   sound stand-in for "this node's throws may depend on dispatch or
//!   external resolution". It folds every file's layout hash and the full
//!   content of every impl-declaring file, so any impl/layout change
//!   invalidates every env-dependent node — mirroring the existing
//!   partition's global `IMPL_SENTINEL`/layout demotions.
//!
//! Firewalled nodes (closed declared `throws`) depend on nothing but their
//! own declaration — matching the propagation firewall in both the retired
//! facts solver and the taint closure.
//!
//! Deliberately coarser than exact (each flagged in the report, sound
//! because every approximation can only ADD invalidation… with one honest
//! exception): the env term over-invalidates; unresolved edges are not
//! traced precisely; and whether the facts layer captures every input that
//! full inference consults (generic instantiation of callee throws,
//! lambda-mediated effects) is exactly what SHADOW MODE exists to test —
//! the inference gate keeps deciding, and any file the gate demotes that
//! this validator called valid is logged loudly as under-invalidation.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use baml_type::throw_facts::FunctionThrowFacts;
use sha2::{Digest, Sha256};

/// Bumped whenever the fingerprint computation changes meaning; folded into
/// every hash so old fingerprints can never validate against a new scheme.
const FP_VERSION: u8 = 1;

/// Rollout mode, from `BAML_THROWS_FINGERPRINTS`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FpMode {
    /// Never compute or compare fingerprints.
    Off,
    /// Compute and compare beside the inference gate; log agreement; the
    /// gate still decides. The default.
    Shadow,
    /// Fingerprints decide; the inference gate is skipped (it still runs
    /// under `BAML_CACHE_VERIFY`, where the reuse plan is disabled anyway).
    Enforce,
}

pub(crate) fn mode() -> FpMode {
    match std::env::var("BAML_THROWS_FINGERPRINTS").as_deref() {
        Ok("off") => FpMode::Off,
        Ok("enforce") => FpMode::Enforce,
        _ => FpMode::Shadow,
    }
}

/// One file's worth of fingerprint inputs, identical in shape on the store
/// side (fresh from the db) and the validate side (clean files from the
/// manifest, dirty files freshly extracted).
pub(crate) struct FileFpInput<'a> {
    /// Project-root-relative path (manifest `rel_path` form).
    pub rel: &'a str,
    /// The file's per-function throw facts.
    pub facts: &'a [FunctionThrowFacts],
    /// The file's type-layout hash (manifest `layout_hash` form).
    pub layout_hash: [u8; 32],
    /// Whether the file declares any impl construct (the `IMPL_SENTINEL`
    /// predicate): its content then contributes to the environment term.
    pub has_impl_construct: bool,
    /// Hash of the file's full content (used only for env contributions of
    /// impl-declaring files).
    pub content_hash: [u8; 32],
}

/// Node-population statistics from one fingerprint computation, for the
/// `BAML_CACHE_DEBUG` summary: how much of the graph is firewalled behind
/// closed `throws` contracts, how much is environment-dependent (unresolved
/// edges ⇒ the impl/layout env term folds in), and how many distinct edge
/// names failed to resolve to a node.
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct FpStats {
    pub nodes: usize,
    pub firewalled: usize,
    pub env_dependent: usize,
    pub unresolved_edges: usize,
}

/// Compute the per-file throws fingerprints for a full project snapshot.
///
/// Pure and deterministic: same inputs (in any order) → same output. The
/// caller supplies EVERY current user file — the environment term and edge
/// resolution are project-global.
pub(crate) fn compute_file_fingerprints(
    files: &[FileFpInput<'_>],
) -> (BTreeMap<String, [u8; 32]>, FpStats) {
    // ── Environment term ────────────────────────────────────────────────
    // Sorted fold over (rel, layout_hash) for every file plus (rel,
    // content_hash) for impl-declaring files.
    let mut env = Sha256::new();
    env.update([FP_VERSION]);
    let mut ordered: Vec<&FileFpInput<'_>> = files.iter().collect();
    ordered.sort_unstable_by(|a, b| a.rel.cmp(b.rel));
    for f in &ordered {
        env.update((f.rel.len() as u64).to_le_bytes());
        env.update(f.rel.as_bytes());
        env.update(f.layout_hash);
        if f.has_impl_construct {
            env.update([1u8]);
            env.update(f.content_hash);
        } else {
            env.update([0u8]);
        }
    }
    let env_hash: [u8; 32] = env.finalize().into();

    // ── Node universe ───────────────────────────────────────────────────
    // Node identity is the solver key. Two functions sharing a key (the
    // retired solver's own aliasing behavior) merge into one node whose
    // base folds both facts — conservative and deterministic.
    struct Node {
        base: Sha256,
        firewalled: bool,
        env_dependent: bool,
        /// Resolved same-package edges, as node indices (post key-dedup).
        deps: BTreeSet<usize>,
        /// Raw edge names, resolved after the universe is built.
        edge_names: BTreeSet<String>,
    }
    let mut key_to_idx: HashMap<&str, usize> = HashMap::new();
    let mut nodes: Vec<Node> = Vec::new();
    // (file rel, node idx) pairs for the final per-file aggregation, plus
    // each file's per-node fact bytes for the base term.
    let mut file_nodes: BTreeMap<&str, Vec<usize>> = BTreeMap::new();

    for f in &ordered {
        for fact in f.facts {
            let key = fact.key.as_str();
            let idx = *key_to_idx.entry(key).or_insert_with(|| {
                let mut base = Sha256::new();
                base.update([FP_VERSION]);
                base.update((key.len() as u64).to_le_bytes());
                base.update(key.as_bytes());
                nodes.push(Node {
                    base,
                    firewalled: true,
                    env_dependent: false,
                    deps: BTreeSet::new(),
                    edge_names: BTreeSet::new(),
                });
                nodes.len() - 1
            });
            let node = &mut nodes[idx];
            // borsh over the whole fact struct: covers direct set, edge
            // list, and the firewall flag byte-exactly.
            let bytes = borsh::to_vec(fact).expect("FunctionThrowFacts is borsh-serializable");
            node.base.update((bytes.len() as u64).to_le_bytes());
            node.base.update(&bytes);
            // A merged node is firewalled only if EVERY occupant is.
            node.firewalled &= fact.has_declared_contract;
            for edge in &fact.call_edges {
                node.edge_names.insert(edge.as_str().to_string());
            }
            file_nodes.entry(f.rel).or_default().push(idx);
        }
    }

    // ── Edge resolution ─────────────────────────────────────────────────
    // A firewalled node propagates nothing inward: no deps, no env.
    for idx in 0..nodes.len() {
        if nodes[idx].firewalled {
            continue;
        }
        let names = std::mem::take(&mut nodes[idx].edge_names);
        for name in &names {
            match key_to_idx.get(name.as_str()) {
                Some(&dep) if dep != idx => {
                    nodes[idx].deps.insert(dep);
                }
                Some(_) => {} // self-edge: SCC handles it trivially
                None => nodes[idx].env_dependent = true,
            }
        }
        nodes[idx].edge_names = names;
    }

    // ── Tarjan SCC (iterative) over resolved edges ──────────────────────
    let n = nodes.len();
    let mut index_of: Vec<Option<u32>> = vec![None; n];
    let mut low: Vec<u32> = vec![0; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut scc_of: Vec<usize> = vec![usize::MAX; n];
    let mut sccs: Vec<Vec<usize>> = Vec::new();
    let mut counter: u32 = 0;
    // Explicit DFS: (node, edge-iterator position)
    for root in 0..n {
        if index_of[root].is_some() {
            continue;
        }
        let mut call: Vec<(usize, Vec<usize>, usize)> = Vec::new();
        let deps: Vec<usize> = nodes[root].deps.iter().copied().collect();
        index_of[root] = Some(counter);
        low[root] = counter;
        counter += 1;
        stack.push(root);
        on_stack[root] = true;
        call.push((root, deps, 0));
        while let Some((v, deps, pos)) = call.pop() {
            if pos < deps.len() {
                let w = deps[pos];
                call.push((v, deps, pos + 1));
                if index_of[w].is_none() {
                    index_of[w] = Some(counter);
                    low[w] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    let wdeps: Vec<usize> = nodes[w].deps.iter().copied().collect();
                    call.push((w, wdeps, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index_of[w].expect("visited"));
                }
            } else {
                if low[v] == index_of[v].expect("visited") {
                    let mut comp = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack underflow");
                        on_stack[w] = false;
                        scc_of[w] = sccs.len();
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    comp.sort_unstable();
                    sccs.push(comp);
                }
                if let Some(&(parent, _, _)) = call.last() {
                    let lv = low[v];
                    low[parent] = low[parent].min(lv);
                }
            }
        }
    }

    // ── Bottom-up fold over the condensation ────────────────────────────
    // Tarjan emits SCCs in reverse topological order of the condensation
    // (every dep's SCC is emitted before its dependents'), so a single
    // forward pass over `sccs` sees all dependency fingerprints first.
    let mut scc_fp: Vec<[u8; 32]> = vec![[0u8; 32]; sccs.len()];
    for (scc_idx, members) in sccs.iter().enumerate() {
        let mut h = Sha256::new();
        h.update([FP_VERSION]);
        let mut member_terms: Vec<[u8; 32]> = Vec::with_capacity(members.len());
        let mut env_dependent = false;
        let mut dep_sccs: BTreeSet<usize> = BTreeSet::new();
        for &m in members {
            member_terms.push(nodes[m].base.clone().finalize().into());
            if !nodes[m].firewalled {
                env_dependent |= nodes[m].env_dependent;
                for &d in &nodes[m].deps {
                    let ds = scc_of[d];
                    if ds != scc_idx {
                        dep_sccs.insert(ds);
                    }
                }
            }
        }
        member_terms.sort_unstable();
        h.update((member_terms.len() as u64).to_le_bytes());
        for t in &member_terms {
            h.update(t);
        }
        if env_dependent {
            h.update([1u8]);
            h.update(env_hash);
        } else {
            h.update([0u8]);
        }
        // Deterministic dep order: fold the dep SCC fingerprints sorted by
        // VALUE, not index (indices are traversal-order-dependent).
        let mut dep_fps: Vec<[u8; 32]> = dep_sccs.iter().map(|&d| scc_fp[d]).collect();
        dep_fps.sort_unstable();
        h.update((dep_fps.len() as u64).to_le_bytes());
        for fp in &dep_fps {
            h.update(fp);
        }
        scc_fp[scc_idx] = h.finalize().into();
    }

    // ── Per-file aggregate ──────────────────────────────────────────────
    let mut out = BTreeMap::new();
    for f in &ordered {
        let mut h = Sha256::new();
        h.update([FP_VERSION]);
        let mut terms: Vec<[u8; 32]> = file_nodes
            .get(f.rel)
            .map(|idxs| idxs.iter().map(|&i| scc_fp[scc_of[i]]).collect())
            .unwrap_or_default();
        terms.sort_unstable();
        h.update((terms.len() as u64).to_le_bytes());
        for t in &terms {
            h.update(t);
        }
        out.insert(f.rel.to_string(), h.finalize().into());
    }

    let mut unresolved: BTreeSet<&str> = BTreeSet::new();
    for node in &nodes {
        if node.firewalled {
            continue;
        }
        for name in &node.edge_names {
            if !key_to_idx.contains_key(name.as_str()) {
                unresolved.insert(name);
            }
        }
    }
    let stats = FpStats {
        nodes: nodes.len(),
        firewalled: nodes.iter().filter(|n| n.firewalled).count(),
        env_dependent: nodes
            .iter()
            .filter(|n| !n.firewalled && n.env_dependent)
            .count(),
        unresolved_edges: unresolved.len(),
    };
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_type::{Name, Ty, TyAttr};

    fn fact(key: &str, throws_string: bool, edges: &[&str], contract: bool) -> FunctionThrowFacts {
        let mut direct = BTreeSet::new();
        if throws_string {
            direct.insert(Ty::String {
                attr: TyAttr::default(),
            });
        }
        FunctionThrowFacts {
            key: Name::new(key),
            direct,
            call_edges: edges.iter().map(|e| Name::new(*e)).collect(),
            has_declared_contract: contract,
        }
    }

    fn input<'a>(
        rel: &'a str,
        facts: &'a [FunctionThrowFacts],
        layout: u8,
        has_impl: bool,
        content: u8,
    ) -> FileFpInput<'a> {
        FileFpInput {
            rel,
            facts,
            layout_hash: [layout; 32],
            has_impl_construct: has_impl,
            content_hash: [content; 32],
        }
    }

    fn compute_file_fingerprints_map(files: &[FileFpInput<'_>]) -> BTreeMap<String, [u8; 32]> {
        compute_file_fingerprints(files).0
    }

    #[test]
    fn stable_and_order_independent() {
        let fa = [fact("a", true, &["b"], false)];
        let fb = [fact("b", false, &[], false)];
        let one = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb, 2, false, 2),
        ]);
        let two = compute_file_fingerprints_map(&[
            input("y", &fb, 2, false, 2),
            input("x", &fa, 1, false, 1),
        ]);
        assert_eq!(one, two);
    }

    #[test]
    fn each_input_perturbation_changes_fp() {
        let fa = [fact("a", true, &["b"], false)];
        let fb = [fact("b", false, &[], false)];
        let base = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb, 2, false, 2),
        ]);

        // Callee's direct throws change → caller fp changes.
        let fb2 = [fact("b", true, &[], false)];
        let v = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb2, 2, false, 2),
        ]);
        assert_ne!(base["x"], v["x"], "callee facts must flow to caller");
        assert_ne!(base["y"], v["y"]);

        // Own edge set changes → own fp changes.
        let fa2 = [fact("a", true, &[], false)];
        let v = compute_file_fingerprints_map(&[
            input("x", &fa2, 1, false, 1),
            input("y", &fb, 2, false, 2),
        ]);
        assert_ne!(base["x"], v["x"]);
        assert_eq!(base["y"], v["y"], "callee is independent of caller");

        // Unresolved edge ⇒ env-dependent ⇒ layout change flows in.
        let fu = [fact("u", false, &["not.a.node"], false)];
        let b1 = compute_file_fingerprints_map(&[input("x", &fu, 1, false, 1)]);
        let b2 = compute_file_fingerprints_map(&[input("x", &fu, 9, false, 1)]);
        assert_ne!(b1["x"], b2["x"], "env-dependent node must fold layout env");

        // …but a fully-resolved, firewall-free local graph ignores env.
        let b1 = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb, 2, false, 2),
        ]);
        let b2 = compute_file_fingerprints_map(&[
            input("x", &fa, 9, false, 1),
            input("y", &fb, 2, false, 2),
        ]);
        assert_eq!(b1["x"], b2["x"], "resolved-only node must not fold env");

        // Impl-file content joins the env term.
        let b1 = compute_file_fingerprints_map(&[input("x", &fu, 1, true, 1)]);
        let b2 = compute_file_fingerprints_map(&[input("x", &fu, 1, true, 7)]);
        assert_ne!(b1["x"], b2["x"]);
    }

    #[test]
    fn firewall_blocks_propagation() {
        // a → b(firewalled) → env-dependent world: b's contract isolates a.
        let fb = [fact("b", true, &["unresolved.thing"], true)];
        let fa = [fact("a", false, &["b"], false)];
        let b1 = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb, 1, false, 1),
        ]);
        let b2 = compute_file_fingerprints_map(&[
            input("x", &fa, 9, false, 1),
            input("y", &fb, 9, false, 1),
        ]);
        assert_eq!(b1["x"], b2["x"], "firewalled callee must not leak env");
        assert_eq!(b1["y"], b2["y"], "firewalled node ignores env entirely");
        // But the firewalled declaration itself changing DOES flow.
        let fb2 = [fact("b", false, &["unresolved.thing"], true)];
        let b3 = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("y", &fb2, 1, false, 1),
        ]);
        assert_ne!(b1["x"], b3["x"]);
    }

    #[test]
    fn scc_joint_fingerprint_is_deterministic_and_shared() {
        // a ↔ b mutual recursion, c depends on the cycle.
        let fa = [fact("a", true, &["b"], false)];
        let fb = [fact("b", false, &["a"], false)];
        let fc = [fact("c", false, &["a"], false)];
        let one = compute_file_fingerprints_map(&[
            input("fa", &fa, 1, false, 1),
            input("fb", &fb, 1, false, 1),
            input("fc", &fc, 1, false, 1),
        ]);
        let two = compute_file_fingerprints_map(&[
            input("fc", &fc, 1, false, 1),
            input("fb", &fb, 1, false, 1),
            input("fa", &fa, 1, false, 1),
        ]);
        assert_eq!(one, two);
        // A change to either cycle member invalidates both members AND c.
        let fb2 = [fact("b", true, &["a"], false)];
        let three = compute_file_fingerprints_map(&[
            input("fa", &fa, 1, false, 1),
            input("fb", &fb2, 1, false, 1),
            input("fc", &fc, 1, false, 1),
        ]);
        assert_ne!(one["fa"], three["fa"]);
        assert_ne!(one["fb"], three["fb"]);
        assert_ne!(one["fc"], three["fc"]);
    }

    #[test]
    fn added_and_removed_definitions_invalidate_referencers() {
        // `a` calls `helper`, which is initially UNRESOLVED (env-dependent).
        let fa = [fact("a", false, &["helper"], false)];
        let before = compute_file_fingerprints_map(&[input("x", &fa, 1, false, 1)]);
        // A new file defines `helper`: the edge now resolves — fp changes.
        let fh = [fact("helper", true, &[], false)];
        let after = compute_file_fingerprints_map(&[
            input("x", &fa, 1, false, 1),
            input("h", &fh, 1, false, 1),
        ]);
        assert_ne!(
            before["x"], after["x"],
            "new definition must re-key referencer"
        );
    }
}
