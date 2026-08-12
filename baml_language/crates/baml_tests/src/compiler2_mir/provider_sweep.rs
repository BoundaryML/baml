//! S16: the pre-cutover differential MIR sweep - rustc's
//! `-Z borrowck=compare` shape, one level down from the S15 type sweep.
//! Lowers EVERY function of the living corpus (`baml_tests/baml_src/`)
//! under BOTH inference providers and diffs the pretty-printed MIR. The
//! snapshot is the ledger: agreements are counted, every differing
//! function is itemized (first differing line pair as the excerpt),
//! every panic is itemized per side. The exit criterion for the S16
//! flip is an empty diff-and-panic section; the burn-down works the
//! itemized entries against the recorded-table gaps (task #59's list)
//! and the ruled type divergences surfacing as template diffs.
//!
//! Top-level lets are NOT diffed yet (no body-level pretty renderer);
//! the header says so - a bounded gap stated, not silently dropped.

use std::fmt::Write;

use baml_compiler2_mir::{
    InferenceProvider, OptLevel, lower_function_with, pretty::display_function,
};
use baml_compiler2_ppir::item_data::{file_functions, function_data, function_source_map};

use crate::type_spec::sweep::{baml_src_dir, read_corpus_files};

/// The verdict machinery runs at `OptLevel::Zero`: the gate proves the
/// INFERENCE seam, and O2's constant folding keys on literal TYPES, so
/// the accepted literal-widening ruling changes which branches fold -
/// a CFG delta that is optimizer policy, not inference disagreement
/// (deferred per the 2026-08-11 ruling: "we can worry about opt
/// later"). O2 is still lowered per function and counted in the header
/// so the deferred surface stays visible.
///
/// One function's verdict under the dual lowering.
enum Verdict {
    Agree,
    /// Every differing line fits a documented ruling's bucket; the label
    /// joins the buckets the diff used.
    Ruled {
        buckets: String,
    },
    Differ {
        excerpt: String,
    },
    Panic {
        side: &'static str,
        message: String,
    },
}

fn lower_pretty(
    db: &baml_project::ProjectDatabase,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    provider: InferenceProvider,
) -> Result<String, String> {
    lower_pretty_at(db, func_loc, provider, OptLevel::Zero)
}

fn lower_pretty_at(
    db: &baml_project::ProjectDatabase,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    provider: InferenceProvider,
    opt: OptLevel,
) -> Result<String, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        display_function(&lower_function_with(db, func_loc, opt, provider))
    }))
    .map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>")
            .to_string()
    })
}

/// The MACHINE-CHECKED ruled-divergence taxonomy, the type sweep's
/// `classify_divergence` discipline one level down: a differing function
/// lands in buckets only when EVERY line of its diff fits a bucket's
/// rule, each bucket names a user ruling, and anything else stays
/// itemized. The walk is a greedy two-pointer alignment because the
/// accepted rulings INSERT lines (receiver seeding adds `load_type`
/// temps), which renumbers every later local - so "same line" means
/// equal modulo local ids, and a renumbering-only pair is admissible
/// exactly because some accepted insertion caused it. Buckets:
///
/// - `receiver-seeding` (ruled 2026-08-11, accept-as-better): hir_ty
///   types receivers TIR never recorded (rest bindings and friends), so
///   MIR's class-arg seeding road fires - `load_type` temps appear, the
///   call gains a `<copy _N>` splice, and the callee frame's type args
///   are correctly seeded where TIR left them unseeded.
/// - `literal-widening` (ruled 2026-08-11; the S7 freshness regime at
///   codegen surfaces): TIR bakes fresh literals into inferred lambda
///   signatures and templates; hir_ty widens (`-> 42` vs `-> int`).
///   Both sides normalize literal tokens in TYPE position to their
///   bases (`const 42_i64` operands are untouched). The normalization
///   is direction-agnostic, so it also nets the RATIFIED (2026-08-11)
///   S12 thrown-literal rule where the roles reverse: hir_ty keeps a
///   thrown literal (`is_type(_, "boom")`) where TIR widened
///   (`is_type(_, string)`) - thrown facts are union members, and the
///   spec's one literal-policy sentence (TYPE_SYSTEM.md line 75) keeps
///   literals as union members.
/// - `thrown-literal` (ratified 2026-08-11): the narrow_bind spelling
///   of the same S12 rule - hir_ty's catch narrows to
///   `Literal(String("boom"), ..)` where TIR narrows to `String {..}`,
///   so catch arms discharge exact error codes.
/// - `void-union` (S15 arm-join ruling): TIR discards statement values
///   (`void`) where hir_ty joins them (`int | void`); interim until
///   void becomes the empty tuple.
/// - `tir-unknown` (ruled 2026-08-11; the S15 tir-uninstantiated family
///   at codegen surfaces): TIR's `unknown` where hir_ty resolved the
///   type - TIR's token wildcard-matches hir's precise type, including
///   line-final throws (subsumes the old throws-precision rule's
///   unknown arm).
/// - `throws-precision`: TIR keeps an unresolved rigid effect var
///   (`throws E`) where hir_ty inferred the effect - the S12 ruling.
/// The unclassifiable line pairs of one diff (empty when the diff
/// classifies), normalized like the classifier sees them - the census
/// groups these across the corpus so remaining families rank by count.
fn residual_pairs(tir: &str, hir: &str) -> Vec<(String, String)> {
    let mut residual = Vec::new();
    for event in aligned_events(tir, hir) {
        match event {
            AlignEvent::Pair(tl, hl) => {
                if classify_pair(&tl, &hl).is_none() {
                    residual.push((
                        strip_local_ids(&tl).trim().to_string(),
                        strip_local_ids(&hl).trim().to_string(),
                    ));
                }
            }
            AlignEvent::HirOnly(hl) => {
                if !seeding_insertion(&hl) {
                    residual.push(("<absent>".into(), strip_local_ids(&hl).trim().to_string()));
                }
            }
            AlignEvent::TirOnly(tl) => {
                residual.push((strip_local_ids(&tl).trim().to_string(), "<absent>".into()));
            }
        }
    }
    residual
}

/// One diff's line events under REAL alignment (Myers via `similar`,
/// insta's own diff engine): equal runs vanish, replace runs pair
/// line-by-line, and pure insertions/deletions surface as one-sided
/// events. The earlier greedy two-pointer desynced on long bodies and
/// classified alignment noise; anchoring on the LCS makes each event
/// the actual local change.
enum AlignEvent {
    Pair(String, String),
    HirOnly(String),
    TirOnly(String),
}

fn aligned_events(tir: &str, hir: &str) -> Vec<AlignEvent> {
    // The diff runs over ID-STRIPPED lines: an accepted insertion
    // renumbers every later local, and diffing raw text would split the
    // declaration block into equal-length replace chunks that defeat
    // insertion detection. Stripping before alignment means pairs that
    // differ only by local ids are EQUAL here - the same admissibility
    // the renumber rule already granted, applied at alignment; the
    // runtime suite stays the behavioral gate for operand wiring.
    let tir_stripped = tir
        .lines()
        .map(|line| strip_local_ids(line))
        .collect::<Vec<_>>()
        .join("\n");
    let hir_stripped = hir
        .lines()
        .map(|line| strip_local_ids(line))
        .collect::<Vec<_>>()
        .join("\n");
    let diff = similar::TextDiff::from_lines(&tir_stripped, &hir_stripped);
    let mut deletes: Vec<String> = Vec::new();
    let mut inserts: Vec<String> = Vec::new();
    let mut events = Vec::new();
    let mut flush =
        |deletes: &mut Vec<String>, inserts: &mut Vec<String>, events: &mut Vec<AlignEvent>| {
            // Seeding-shaped inserts peel off FIRST: a replace run holding
            // both an accepted insertion and renumbered lines must not let
            // the insertion shift the positional pairing of the rest.
            if inserts.len() > deletes.len() {
                let mut surplus = inserts.len() - deletes.len();
                inserts.retain(|line| {
                    if surplus > 0 && seeding_insertion(line) {
                        surplus -= 1;
                        events.push(AlignEvent::HirOnly(line.clone()));
                        false
                    } else {
                        true
                    }
                });
            }
            let paired = deletes.len().min(inserts.len());
            for (tl, hl) in deletes.drain(..paired).zip(inserts.drain(..paired)) {
                events.push(AlignEvent::Pair(tl, hl));
            }
            events.extend(deletes.drain(..).map(AlignEvent::TirOnly));
            events.extend(inserts.drain(..).map(AlignEvent::HirOnly));
        };
    for change in diff.iter_all_changes() {
        let line = change.value().trim_end_matches('\n').to_string();
        match change.tag() {
            similar::ChangeTag::Equal => flush(&mut deletes, &mut inserts, &mut events),
            similar::ChangeTag::Delete => deletes.push(line),
            similar::ChangeTag::Insert => inserts.push(line),
        }
    }
    flush(&mut deletes, &mut inserts, &mut events);
    events
}

fn hir_stripped_lines(hir: &str) -> std::collections::BTreeSet<String> {
    hir.lines().map(strip_local_ids).collect()
}

fn classify_diff(tir: &str, hir: &str) -> Option<String> {
    let mut buckets: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    let mut renumber_only = false;
    // Swapped runtime tests (canonical arm ORDER, ruled 2026-08-11):
    // failing is_type pairs collect and admit at the end when the two
    // sides' payload multisets are equal - the same tests in a
    // different order over disjoint classes.
    let mut swapped_tir: Vec<String> = Vec::new();
    let mut swapped_hir: Vec<String> = Vec::new();
    // TIR-only structural lines (decls, load_type temps, block glue)
    // collect and admit at the end ONLY when the diff also shows the
    // canonical-absorption pair (ruled 2026-08-11): TIR's unabsorbed
    // `Super | Sub` union drives extra match machinery hir's absorbed
    // union never emits.
    let mut tir_only_structural = 0usize;
    let mut absorption_pair = false;
    let events = aligned_events(tir, hir);
    // virtual-dispatch (the proper-dyn ruling, "keep hir obviously"):
    // TIR devirtualizes an interface call into an is_type ladder over
    // the implementing classes; hir_ty dispatches through the interface
    // slot. The ladder's lines admit ONLY when the diff shows the
    // anchor pair (TIR's arm test against hir's virtual_call), and the
    // TIR-only calls must name the anchor's member.
    let dispatch_member: Option<String> = events.iter().find_map(|event| match event {
        AlignEvent::Pair(tl, hl) => virtual_call_anchor(tl, hl),
        _ => None,
    });
    for event in events {
        match event {
            AlignEvent::Pair(tl, hl) => {
                let swappable = |s: &str| {
                    let s = s.trim_start();
                    s.starts_with("_ = is_type(") || s.starts_with("_ = call const fn ")
                };
                let t_test = swappable(&tl);
                let h_test = swappable(&hl);
                match classify_pair(&tl, &hl) {
                    Some(Some(bucket)) => {
                        buckets.insert(bucket);
                    }
                    Some(None) => renumber_only = true,
                    None if t_test && h_test => {
                        swapped_tir.push(normalize_line(&tl));
                        swapped_hir.push(normalize_line(&hl));
                    }
                    None => {
                        if virtual_call_anchor(&tl, &hl).is_some()
                            || (dispatch_member.is_some() && dispatch_narrow_pair(&tl, &hl))
                        {
                            buckets.insert("virtual-dispatch");
                        } else if let Some(true) = load_type_superset_pair(&tl, &hl) {
                            absorption_pair = true;
                            buckets.insert("canonical-absorption");
                        } else {
                            return None;
                        }
                    }
                }
            }
            AlignEvent::HirOnly(hl) => {
                if !seeding_insertion(&hl) {
                    return None;
                }
                buckets.insert("receiver-seeding");
            }
            AlignEvent::TirOnly(tl) => {
                let trimmed = tl.trim_start();
                // proven-coverage (ruled 2026-08-11, "lean hir"): TIR
                // emits an always-true arm test hir's claiming machinery
                // proved redundant (interface-implements coverage of a
                // union scrutinee) - TIR-only TEST lines admit as their
                // own cause; the runtime suite under the hir provider is
                // the behavioral gate for the removed test.
                if trimmed.starts_with("branch copy _ -> ")
                    || trimmed.starts_with("_ = is_type(")
                    || trimmed == "let _: bool"
                {
                    buckets.insert("proven-coverage");
                    tir_only_structural += 1;
                } else if structural_line(&tl) {
                    tir_only_structural += 1;
                } else if let Some(member) = &dispatch_member
                    && (trimmed.starts_with("_ = narrow_bind copy _ as Class(")
                        || (trimmed.starts_with("_ = call const fn ")
                            && trimmed.contains(&format!(".{member}(copy _"))))
                {
                    // The devirt ladder's per-class arms: the Class
                    // narrow and the concrete call to the SAME member
                    // the anchor dispatches virtually.
                    buckets.insert("virtual-dispatch");
                    tir_only_structural += 1;
                } else if tl.trim_start().starts_with("_ = ")
                    && hir_stripped_lines(hir).contains(&tl)
                {
                    // TIR duplicates statements (calls, constructions,
                    // copies) across the dispatch arms its unabsorbed
                    // union forces; the SAME line exists in hir's output
                    // - duplication, never disappearance (a statement
                    // with no hir counterpart stays itemized).
                    tir_only_structural += 1;
                } else {
                    return None;
                }
            }
        }
    }
    if !swapped_tir.is_empty() {
        let mut t_sorted = swapped_tir.clone();
        let mut h_sorted = swapped_hir.clone();
        t_sorted.sort_unstable();
        h_sorted.sort_unstable();
        if t_sorted != h_sorted {
            return None;
        }
        buckets.insert("canonical-order");
    }
    if tir_only_structural > 0
        && !absorption_pair
        && !buckets.contains("proven-coverage")
        && !buckets.contains("virtual-dispatch")
    {
        return None;
    }
    // A pure-renumbering pair is only admissible as the CONSEQUENCE of
    // an accepted insertion or removal; renumbering with no cause stays
    // itemized.
    if renumber_only
        && !buckets.contains("receiver-seeding")
        && !absorption_pair
        && !buckets.contains("proven-coverage")
        && !buckets.contains("virtual-dispatch")
    {
        return None;
    }
    (!buckets.is_empty()).then(|| buckets.iter().copied().collect::<Vec<_>>().join("+"))
}

/// The top-level parameter list of a fn-typed DECL line, when the line
/// is one (`let _: (a, b) -> ...`).
fn fn_params(line: &str) -> Option<Vec<String>> {
    let rest = line.trim_start().strip_prefix("let _: (")?;
    let mut depth = 0usize;
    let mut end = None;
    for (index, c) in rest.char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' if depth == 0 => {
                end = Some(index);
                break;
            }
            // `->` arrows put a stray `>` at depth 0.
            ')' | '>' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let end = end?;
    if !rest[end..].starts_with(") -> ") {
        return None;
    }
    let mut params = Vec::new();
    let (mut depth, mut last) = (0usize, 0usize);
    let list = &rest[..end];
    for (index, c) in list.char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                params.push(list[last..index].trim().to_string());
                last = index + 1;
            }
            _ => {}
        }
    }
    if !list[last..].trim().is_empty() {
        params.push(list[last..].trim().to_string());
    }
    Some(params)
}

/// Everything after a fn decl's parameter list (return + throws).
fn after_params(line: &str) -> String {
    line.split(") -> ")
        .skip(1)
        .collect::<Vec<_>>()
        .join(") -> ")
}

/// The arm set of a `switch copy _ [...]` line, when it is one.
fn switch_arms(line: &str) -> Option<std::collections::BTreeSet<String>> {
    let line = line.trim_start();
    let rest = line.strip_prefix("switch copy _ [")?;
    let end = rest.find(']')?;
    Some(
        rest[..end]
            .split(", ")
            .map(|arm| arm.trim().to_string())
            .collect(),
    )
}

/// Replaces the type region after the LAST `: ` with `_ELEM_` when it
/// ends with `[]` - the empty-list pair's hir side carries the adopted
/// element there.
fn regex_free_elem_placeholder(line: &str) -> String {
    let Some(at) = line.rfind(": ") else {
        return line.to_string();
    };
    let (head, tail) = line.split_at(at + 2);
    if tail.trim_end_matches(';').ends_with("[]") {
        let suffix = if tail.ends_with(';') { ";" } else { "" };
        format!("{head}_ELEM_{suffix}")
    } else {
        line.to_string()
    }
}

/// Strips ONE wrapping paren layer from the region after `: ` (the
/// chain-peel union parenthesizes the fn member; the bare decl does
/// not).
fn strip_one_paren_layer(line: &str) -> Option<String> {
    let at = line.find(": (")?;
    let inner_start = at + ": (".len();
    let mut depth = 1usize;
    for (index, c) in line[inner_start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let close = inner_start + index;
                    return Some(format!(
                        "{}: {}{}",
                        &line[..at],
                        &line[inner_start..close],
                        &line[close + 1..]
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// A load_type pair where the TIR payload's members are a STRICT
/// superset of hir's (the unabsorbed `Super | Sub` vs the absorbed
/// `Super` - the ruled canonical absorption).
fn load_type_superset_pair(tir: &str, hir: &str) -> Option<bool> {
    let t = tir.trim_start().strip_prefix("_ = load_type(")?;
    let h = hir.trim_start().strip_prefix("_ = load_type(")?;
    let t_members: std::collections::BTreeSet<&str> = t
        .trim_end_matches(");")
        .split(" | ")
        .map(str::trim)
        .collect();
    let h_members: std::collections::BTreeSet<&str> = h
        .trim_end_matches(");")
        .split(" | ")
        .map(str::trim)
        .collect();
    Some(h_members.is_subset(&t_members) && h_members.len() < t_members.len())
}

/// TIR-only lines the canonical-absorption allowance admits: pure
/// structure (decls, load_type temps, block glue) - never operations.
fn structural_line(line: &str) -> bool {
    let line = line.trim();
    line.is_empty()
        || line == "}"
        || line == "unreachable;"
        || line.starts_with("let _:")
        || line.starts_with("_ = load_type(")
        || line.starts_with("bb")
        || line.starts_with("goto -> ")
        || line.starts_with("switch ")
        || line.starts_with("_ = type_tag(")
        || line.starts_with("_ = is_type(")
        || line.starts_with("_ = copy _;")
        || line.starts_with("branch ")
}

/// A line the receiver-seeding ruling INSERTS on the hir side: a
/// `load_type` temp assignment or its `type`-typed local declaration.
fn seeding_insertion(line: &str) -> bool {
    let line = line.trim_start();
    (line.starts_with("let _") && line.trim_end().ends_with(": type"))
        || (line.starts_with('_') && line.contains(" = load_type("))
}

/// One aligned differing line pair. `Some(Some(bucket))` = fits a ruled
/// bucket; `Some(None)` = equal modulo local ids / accepted splices
/// (renumbering fallout); `None` = does not fit - the function stays
/// itemized.
fn classify_pair(tir: &str, hir: &str) -> Option<Option<&'static str>> {
    let t_ids = strip_local_ids(tir);
    let h_ids = strip_local_ids(&strip_type_arg_splice(hir));
    if t_ids == h_ids {
        return Some(if hir.contains("<copy _") && !tir.contains("<copy _") {
            Some("receiver-seeding")
        } else {
            None
        });
    }
    let t_norm = normalize_line(&t_ids);
    let h_norm = normalize_line(&h_ids);
    if t_norm == h_norm {
        return Some(Some("literal-widening"));
    }
    if t_norm.contains("unknown")
        && wildcard_match(&t_norm, "unknown", &h_norm, &|filled| !filled.is_empty())
    {
        return Some(Some("tir-unknown"));
    }
    if t_norm.contains("void")
        && wildcard_match(&t_norm, "void", &h_norm, &|filled| {
            filled == "void" || filled.ends_with(" | void")
        })
    {
        return Some(Some("void-union"));
    }
    // self-param-render (S15 ruling: fn renders SHOW the self param;
    // TIR strips it): tir's param list equals hir's minus its first.
    if let (Some(t_params), Some(h_params)) = (fn_params(&t_norm), fn_params(&h_norm))
        && after_params(&t_norm) == after_params(&h_norm)
        && ((h_params.len() == t_params.len() + 1 && h_params[1..] == t_params[..])
            || (t_params.len() == h_params.len()
                && t_params != h_params
                && t_params
                    .iter()
                    .zip(&h_params)
                    .all(|(t, h)| t == h || t == "Self")))
    {
        return Some(Some("self-param-render"));
    }
    // tir-uninstantiated (S15 ruled family): a decl where TIR shows bare
    // generic vars and hir the concrete instantiation. Coarse by design
    // (the census stays the review surface): single-uppercase tokens on
    // the TIR side only.
    if t_norm.trim_start().starts_with("let _:") {
        let t_tokens = generic_tokens(&t_norm);
        let h_tokens = generic_tokens(&h_norm);
        if h_tokens.is_subset(&t_tokens) && h_tokens.len() < t_tokens.len() {
            return Some(Some("tir-uninstantiated"));
        }
    }
    // The is_type spelling of the same family: TIR renders an
    // unresolved projection placeholder (`#1`, `(#0 as I).Label`) where
    // hir_ty resolved the projection to its concrete type.
    // On the UN-normalized forms: literal normalization rewrites the
    // placeholder's digit (`#1` -> `#int`) and would hide it.
    if let (Some(t_at), Some(h_at)) = (
        t_ids.find("is_type(copy _, "),
        h_ids.find("is_type(copy _, "),
    ) && t_ids[..t_at] == h_ids[..h_at]
        && has_projection_var(&t_ids[t_at..])
        && !has_projection_var(&h_ids[h_at..])
    {
        return Some(Some("tir-uninstantiated"));
    }
    // effect-arg-precision: load_type error-set pairs in a subset
    // relation (hir's written both-member set vs TIR's under-join, or
    // the reverse) - every differing member must be an Error class.
    if let (Some(t), Some(h)) = (
        t_norm.trim_start().strip_prefix("_ = load_type("),
        h_norm.trim_start().strip_prefix("_ = load_type("),
    ) {
        let t_set: std::collections::BTreeSet<&str> = t
            .trim_end()
            .trim_end_matches(");")
            .split(" | ")
            .map(str::trim)
            .collect();
        let h_set: std::collections::BTreeSet<&str> = h
            .trim_end()
            .trim_end_matches(");")
            .split(" | ")
            .map(str::trim)
            .collect();
        if t_set != h_set
            && (t_set.is_subset(&h_set) || h_set.is_subset(&t_set))
            && t_set
                .symmetric_difference(&h_set)
                .all(|member| member.ends_with("Error"))
        {
            return Some(Some("effect-arg-precision"));
        }
    }
    // bigint-context (ruled 2026-08-11): an int expression in a bigint
    // slot KEEPS int - TS has no int-to-bigint literal adoption (the `n`
    // suffix exists for exactly that), mixing is legal through the
    // declared `Add<int>` ops rule, and the VM's mixed opcodes take the
    // int operand directly, so TIR's contextual `bigint` describes an
    // allocation that never happens.
    if t_norm.contains("bigint")
        && wildcard_match(&t_norm, "bigint", &h_norm, &|filled| {
            filled == "int" || filled == "bigint"
        })
    {
        return Some(Some("bigint-context"));
    }
    if thrown_literal_narrow_pair(&t_ids, &h_ids) {
        return Some(Some("thrown-literal"));
    }
    // tir-never-test (ruled 2026-08-11): TIR records a catch-arm
    // pattern's type as `never` (its residual-subtraction artifact on an
    // unknown throw set) and emits an always-false arm test at O0; hir_ty
    // records the pattern the source wrote. The optimized pipeline lowers
    // these arms through tag switches, which is why the dead test never
    // shipped. Scoped to the is_type spelling with `never` on the TIR
    // side only.
    if let (Some(t_at), Some(h_at)) = (
        t_norm.find("is_type(copy _, "),
        h_norm.find("is_type(copy _, "),
    ) && t_norm[..t_at] == h_norm[..h_at]
        && t_norm[t_at..].trim_end() == "is_type(copy _, never);"
        && h_norm[h_at..].trim_end().ends_with(");")
    {
        return Some(Some("tir-never-test"));
    }
    if throws_rigid_var_pair(&t_norm, &h_norm) {
        return Some(Some("throws-precision"));
    }
    // written-throws (the written-ascription ruling, is_type spelling):
    // the source writes `throws unknown` in an `is` pattern; hir_ty
    // records the written form where TIR rewrote it to `never`.
    if t_norm.contains("is_type(copy _, ")
        && h_norm.contains("throws unknown")
        && h_norm.replace("throws unknown", "throws never") == t_norm
    {
        return Some(Some("written-throws"));
    }
    // Switch ARM order (canonical-order ruling, switch spelling): an
    // exhaustive tag switch's arms are order-independent; equal arm
    // multisets admit.
    if let (Some(t_arms), Some(h_arms)) = (switch_arms(&t_norm), switch_arms(&h_norm))
        && t_arms == h_arms
    {
        return Some(Some("canonical-order"));
    }
    // empty-list-adoption (hir ahead; the sibling/expectation adoption
    // rulings): TIR types an empty list's element as `never` where
    // hir_ty adopted the context's element.
    if t_norm.contains(": never[]")
        && t_norm.replacen(": never[]", ": _ELEM_", 1) == regex_free_elem_placeholder(&h_norm)
    {
        return Some(Some("empty-list-adoption"));
    }
    if t_norm.contains(" | never[]") {
        let stripped = t_norm.replace(" | never[]", "");
        if stripped == h_norm || drop_redundant_array_parens(&stripped) == h_norm {
            return Some(Some("empty-list-adoption"));
        }
    }
    // chain-callee-peel (ruled 2026-08-11, "do what TS does"): an
    // optional-chain link's callee temp types as the PEELED function
    // (TS short-circuit semantics - intermediate links see the
    // non-null type); TIR includes the link's null. Scoped to fn-typed
    // decls where stripping one `null | ` from the TIR side matches.
    if t_norm.contains("->") && (t_norm.contains("null | ") || t_norm.contains(" | null")) {
        let head = t_norm.replacen("null | ", "", 1);
        // BTreeSet canonical order sorts `(` before `n`, so the peeled
        // null can land at the TAIL of the canonicalized union.
        let tail = match t_norm.rfind(" | null") {
            Some(at) => format!("{}{}", &t_norm[..at], &t_norm[at + " | null".len()..]),
            None => t_norm.clone(),
        };
        for stripped in [head, tail] {
            let unparen = strip_one_paren_layer(&stripped);
            if stripped == h_norm || unparen.as_deref() == Some(h_norm.as_str()) {
                return Some(Some("chain-callee-peel"));
            }
        }
    }
    // virtual-field-index (the proper-dyn ruling): TIR reads a virtual
    // field by NAME through an upcast; hir_ty reads by index through
    // the declaring interface's realized view - same slot, the render
    // MIR's runtime resolver actually keys on.
    if virtual_field_index_pair(&t_ids, &h_ids) {
        return Some(Some("virtual-field-index"));
    }
    // static-dispatch (the proper-dyn ruling's converse): where the
    // receiver's concrete type is statically known, hir_ty resolves the
    // interface member to the impl's method (InterfaceConcreteMethod)
    // and MIR emits the direct call TIR dispatched virtually.
    if static_dispatch_pair(&t_ids, &h_ids) {
        return Some(Some("static-dispatch"));
    }
    None
}

/// The devirt-ladder ANCHOR pair: TIR tests an arm's class where hir
/// dispatches through the interface slot. Returns the dispatched
/// member's name.
fn virtual_call_anchor(tir: &str, hir: &str) -> Option<String> {
    tir.trim_start().strip_prefix("_ = is_type(copy _, ")?;
    let h = hir.trim_start().strip_prefix("_ = virtual_call ")?;
    let member = h.split(" as ").next()?;
    (!member.is_empty()).then(|| member.to_string())
}

/// The ladder's narrow pair: TIR narrows to the arm's CLASS where hir
/// narrows to the dispatch INTERFACE (same place, same successor shape).
fn dispatch_narrow_pair(tir: &str, hir: &str) -> bool {
    let (Some(t), Some(h)) = (
        tir.trim_start().strip_prefix("_ = narrow_bind copy _ as "),
        hir.trim_start().strip_prefix("_ = narrow_bind copy _ as "),
    ) else {
        return false;
    };
    t.starts_with("Class(") && h.starts_with("Interface(")
}

/// TIR `_ = copy _.name#K as Type;` vs hir `_ = copy _.K;` - the two
/// renders of the same virtual-field slot read.
fn virtual_field_index_pair(tir: &str, hir: &str) -> bool {
    let (Some(t_at), Some(h_at)) = (tir.find("= copy _."), hir.find("= copy _.")) else {
        return false;
    };
    if tir[..t_at] != hir[..h_at] {
        return false;
    }
    let t_rest = &tir[t_at + "= copy _.".len()..];
    let h_rest = &hir[h_at + "= copy _.".len()..];
    let Some(hash) = t_rest.find('#') else {
        return false;
    };
    let t_index: &str = t_rest[hash + 1..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    !t_index.is_empty()
        && t_rest[hash + 1 + t_index.len()..].starts_with(" as ")
        && h_rest.trim_end() == format!("{t_index};")
}

/// TIR `_ = virtual_call M as I(ARGS` vs hir `_ = call const fn P(ARGS`
/// where the concrete path P routes through I to the SAME member M.
fn static_dispatch_pair(tir: &str, hir: &str) -> bool {
    let (Some(t), Some(h)) = (
        tir.trim_start().strip_prefix("_ = virtual_call "),
        hir.trim_start().strip_prefix("_ = call const fn "),
    ) else {
        return false;
    };
    let Some((member, t_rest)) = t.split_once(" as ") else {
        return false;
    };
    let Some((iface, t_args)) = t_rest.split_once('(') else {
        return false;
    };
    let Some((path, h_args)) = h.split_once('(') else {
        return false;
    };
    t_args == h_args && path.ends_with(&format!(".{member}")) && path.contains(iface)
}

/// Drops parens around an array's parenthesized element when the
/// element needs none (`(int[])[]` -> `int[][]`): no union, no fn
/// arrow, no tuple comma inside.
fn drop_redundant_array_parens(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let inner = &line[i + 1..j - 1];
            if depth == 0
                && line[j..].starts_with('[')
                && !inner.contains('|')
                && !inner.contains("->")
                && !inner.contains(',')
            {
                out.push_str(inner);
                i = j;
                continue;
            }
        }
        out.push(line.as_bytes()[i] as char);
        i += 1;
    }
    out
}

/// Bare single-uppercase generic tokens on a line (`T`, `U`, `E`).
fn generic_tokens(line: &str) -> std::collections::BTreeSet<char> {
    let mut out = std::collections::BTreeSet::new();
    let mut prev_alnum = false;
    let chars: Vec<char> = line.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && !prev_alnum
            && chars
                .get(index + 1)
                .is_none_or(|&next| !(next.is_alphanumeric() || next == '_' || next == '.'))
        {
            out.insert(c);
        }
        prev_alnum = c.is_alphanumeric() || c == '_' || c == '.';
    }
    out
}

/// Whether a rendered type carries a TIR projection placeholder (`#0`).
fn has_projection_var(text: &str) -> bool {
    text.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'#' && w[1].is_ascii_digit())
}

/// The narrow_bind spelling of the ratified thrown-literal rule: both
/// sides narrow the same place, TIR to a base-type template, hir_ty to
/// a literal of that base.
fn thrown_literal_narrow_pair(tir: &str, hir: &str) -> bool {
    if !tir.contains("narrow_bind") || !hir.contains("narrow_bind") {
        return false;
    }
    let (Some(t_as), Some(h_as)) = (tir.find(" as "), hir.find(" as ")) else {
        return false;
    };
    if tir[..t_as] != hir[..h_as] {
        return false;
    }
    let t_ty = tir[t_as + " as ".len()..].trim_start();
    let h_ty = hir[h_as + " as ".len()..].trim_start();
    for base in ["String", "Int", "Bigint", "Float", "Bool"] {
        // Thrown-literal direction: hir narrows to the literal code TIR
        // widened. Written-ascription direction: hir records the WRITTEN
        // base where TIR narrowed to the literal. Both ruled.
        if (t_ty.starts_with(base) && h_ty.starts_with(&format!("Literal({base}(")))
            || (h_ty.starts_with(base) && t_ty.starts_with(&format!("Literal({base}(")))
        {
            return true;
        }
    }
    false
}

/// Replaces every local id (`_12`) with `_` so renumbering compares
/// equal; ids appear only in this shape in the pretty-printer.
fn strip_local_ids(line: &str) -> String {
    // Local ids (`_12`) and BLOCK ids (`bb7`) both renumber under
    // accepted insertions/removals; both strip to their prefix.
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        if c == '_' {
            while i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                i += 1;
            }
        } else if c == 'b'
            && i + 1 < chars.len()
            && chars[i + 1] == 'b'
            && i + 2 < chars.len()
            && chars[i + 2].is_ascii_digit()
            && (i == 0 || !chars[i - 1].is_alphanumeric())
        {
            out.push('b');
            i += 1;
            while i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                i += 1;
            }
        }
        i += 1;
    }
    out
}

/// Removes a `<copy _N, ...>` type-arg splice after a callee name - the
/// receiver-seeding ruling's call-site delta.
fn strip_type_arg_splice(line: &str) -> String {
    let Some(start) = line.find("<copy _") else {
        return line.to_string();
    };
    let Some(end) = line[start..].find('>') else {
        return line.to_string();
    };
    format!("{}{}", &line[..start], &line[start + end + 1..])
}

/// Normalizes literal tokens in TYPE position to their base-type names
/// on BOTH sides (the hir side has none after widening, so this is
/// symmetric). `const 42_i64` operands are untouched: only bare tokens
/// qualify, and constants carry the `const ` prefix and a suffix.
/// The stdlib json union's definition text - TIR renders the alias
/// EXPANDED one level deeper than hir_ty's nominal render (the S15
/// alias-nominal ruled family); collapsing the definition to the name on
/// both sides nets the difference.
const JSON_DEF: &str =
    "int | float | string | bool | null | baml.json.json[] | map<string, baml.json.json>";

fn collapse_json_alias(line: &str) -> String {
    // Joint fixpoint: collapsing an inner expansion leaves the outer one
    // spelled with parens in list position (`(baml.json.json)[]`), and
    // stripping those parens re-creates the definition form - so the two
    // rewrites must interleave until neither applies.
    let mut out = line.to_string();
    loop {
        let before = out.len();
        out = out.replace(JSON_DEF, "baml.json.json");
        out = out.replace("(baml.json.json)", "baml.json.json");
        if out.len() == before {
            break;
        }
    }
    out
}

/// Collapses duplicate adjacent union members after literal
/// normalization (`bool | true` normalizes to `bool | bool`, which IS
/// `bool` - TIR renders such unions uncollapsed; the canonical algebra
/// and hir_ty collapse them).
fn dedup_union_members(line: &str) -> String {
    let mut out = line.to_string();
    for base in ["int", "float", "string", "bool", "bigint", "null"] {
        let dup = format!("{base} | {base}");
        while out.contains(&dup) {
            out = out.replace(&dup, base);
        }
    }
    out
}

/// THE central line normalizer (ruled 2026-08-11: "normalize nigh
/// everywhere, and this system should be centralized"): every aligned
/// pair passes through ONE pipeline - local ids strip, literal tokens in
/// type position map to bases, the json alias collapses (canonical AND
/// Debug-template spellings), duplicate union members dedup, and union
/// TYPE REGIONS canonicalize (member sort per the S15 canonical-order
/// ruling + variant absorption per the canonical-union ruling) wherever
/// a type can appear: load_type / is_type operands, local decls, array
/// annotations, narrow_bind templates.
fn normalize_line(line: &str) -> String {
    let out = strip_local_ids(line);
    // json collapses FIRST: literal normalization rewrites quoted tokens
    // (`"json"` -> string) and would break the Debug-blob matcher.
    let out = collapse_json_alias_debug(&out);
    let out = collapse_json_alias(&out);
    let out = normalize_quoted_spans(&out);
    let out = normalize_type_literals(&out);
    // Tokenwise literal mapping leaves a sign artifact on negatives.
    let out = out
        .replace("-int", "int")
        .replace("-float", "float")
        .replace("-bigint", "bigint");
    let out = dedup_union_members(&out);
    canonicalize_type_regions(&out)
}

/// Type-region openers and how each region ENDS. One table, one
/// canonicalizer - adding a position means adding a row.
const TYPE_REGIONS: &[(&str, RegionEnd)] = &[
    ("load_type(", RegionEnd::MatchingParen),
    ("is_type(copy _, ", RegionEnd::MatchingParen),
    ("let _: ", RegionEnd::LineTail),
    ("]: ", RegionEnd::Semicolon),
    (" as ", RegionEnd::Arrow),
];

#[derive(Clone, Copy)]
enum RegionEnd {
    MatchingParen,
    /// Up to `;` (array-literal annotations: `[...]: T;`).
    Semicolon,
    /// Up to ` -> [` (narrow_bind templates).
    Arrow,
    /// To end of line, minus a trailing ` // comment`.
    LineTail,
}

fn canonicalize_type_regions(line: &str) -> String {
    let mut out = line.to_string();
    for &(opener, end) in TYPE_REGIONS {
        let Some(start) = out.find(opener) else {
            continue;
        };
        let region_start = start + opener.len();
        let region_len = match end {
            RegionEnd::MatchingParen => {
                let mut depth = 0usize;
                let mut len = None;
                for (index, c) in out[region_start..].char_indices() {
                    match c {
                        '(' | '<' | '[' => depth += 1,
                        ')' if depth == 0 => {
                            len = Some(index);
                            break;
                        }
                        // `->` arrows put a stray `>` at depth 0.
                        ')' | '>' | ']' => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                }
                match len {
                    Some(len) => len,
                    None => continue,
                }
            }
            RegionEnd::Semicolon => match out[region_start..].find(';') {
                Some(len) => len,
                None => continue,
            },
            RegionEnd::Arrow => match out[region_start..].find(" -> [") {
                Some(len) => len,
                None => continue,
            },
            RegionEnd::LineTail => {
                let tail = &out[region_start..];
                tail.find(" //").unwrap_or(tail.len())
            }
        };
        let region = out[region_start..region_start + region_len].to_string();
        let canonical = if matches!(end, RegionEnd::Semicolon) && region.ends_with("[]") {
            // The array-literal annotation position: the whole region is
            // the ARRAY type and the printer attaches `[]` to the last
            // union member without parens - lift the suffix, canonicalize
            // the element union, re-suffix.
            format!(
                "{}[]",
                canonicalize_union(region.strip_suffix("[]").expect("checked"))
            )
        } else {
            canonicalize_union(&region)
        };
        out = format!(
            "{}{}{}",
            &out[..region_start],
            canonical,
            &out[region_start + region_len..]
        );
    }
    out
}

/// Sorts a union's DEPTH-0 members, absorbs `X.Y` into a sibling `X`
/// (enum variants into their enum), dedups. Non-union regions pass
/// through untouched.
fn canonicalize_union(payload: &str) -> String {
    // A fn type's union RETURN is unparenthesized: a depth-0 `->` means
    // depth-0 `|`s may belong to the return type, so the region stays
    // verbatim (fn-typed decls compare structurally).
    {
        let mut depth = 0usize;
        let bytes = payload.as_bytes();
        for (index, &b) in bytes.iter().enumerate() {
            match b {
                b'<' | b'[' | b'(' => depth += 1,
                b'>' if index > 0 && bytes[index - 1] == b'-' && depth == 0 => {
                    return payload.to_string();
                }
                b'>' | b']' | b')' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    let mut members: Vec<&str> = Vec::new();
    let (mut depth, mut last) = (0usize, 0usize);
    for (index, c) in payload.char_indices() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                members.push(payload[last..index].trim());
                last = index + 1;
            }
            _ => {}
        }
    }
    members.push(payload[last..].trim());
    if members.len() < 2 {
        return payload.to_string();
    }
    // A BTreeSet IS the canonical form: sorted and deduplicated by
    // construction; absorption then filters variant members whose enum
    // is present.
    let set: std::collections::BTreeSet<&str> = members.into_iter().collect();
    set.iter()
        .copied()
        .filter(|member| {
            !set.iter().any(|other| {
                member
                    .strip_prefix(other)
                    .is_some_and(|rest| rest.starts_with('.'))
            })
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The Debug-template spelling of the json alias expansion (narrow_bind
/// templates print `TyTemplate` Debug). The attr blob is uniform, so it
/// compresses first; both observed member orders collapse.
fn collapse_json_alias_debug(line: &str) -> String {
    const ATTR: &str = "TyAttr { sap_parse_without_null: Unset, sap_pending_never: Unset, sap_in_progress_never: Unset }";
    if !line.contains(ATTR) {
        return line.to_string();
    }
    let mut out = line.replace(ATTR, "@A");
    const Q: &str = r#"QualifiedTypeName { pkg: Dep("baml"), namespace: ["json"], name: "json" }"#;
    let json = format!("TypeAlias({Q}, @A)");
    // Bracket-scan every `Union([...], @A)` whose member list is the json
    // expansion (signature members present, any order) and collapse it to
    // the alias - innermost first via repeated passes.
    loop {
        let mut changed = false;
        let mut search = 0usize;
        while let Some(rel) = out[search..].find("Union([") {
            let start = search + rel;
            let list_start = start + "Union([".len();
            let mut depth = 1usize;
            let mut list_end = None;
            for (index, c) in out[list_start..].char_indices() {
                match c {
                    '[' | '(' | '{' => depth += 1,
                    ']' if depth == 1 => {
                        list_end = Some(list_start + index);
                        break;
                    }
                    ']' | ')' | '}' => depth -= 1,
                    _ => {}
                }
            }
            let Some(list_end) = list_end else { break };
            let members = &out[list_start..list_end];
            let is_json = members.contains("Int { attr: @A }")
                && members.contains("Float { attr: @A }")
                && members.contains("Null { attr: @A }")
                && members.contains(&format!("List({json}, @A)"))
                && members.contains(&format!("value: {json}"))
                && !members[..members.len()].contains("Union([");
            let close = ", @A)";
            if is_json && out[list_end..].starts_with(&format!("]{close}")) {
                let end = list_end + 1 + close.len();
                out = format!("{}{}{}", &out[..start], json, &out[end..]);
                changed = true;
                search = 0;
            } else {
                search = start + "Union([".len();
            }
        }
        if !changed {
            break;
        }
    }
    out
}

/// Quoted spans (string literals, spaces included) normalize to
/// `string` in one pre-pass; the tokenwise pass below cannot see a
/// multi-word literal.
fn normalize_quoted_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
            }
            out.push_str("string");
        } else {
            out.push(c);
        }
    }
    out
}

fn normalize_type_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for (index, token) in split_inclusive_tokens(line) {
        let preceded_by_const = line[..index].trim_end().ends_with("const");
        if preceded_by_const {
            out.push_str(token);
            continue;
        }
        out.push_str(literal_base(token).unwrap_or(token));
    }
    out
}

/// Tokenizes into maximal runs of token chars and single non-token
/// chars, keeping byte offsets.
fn split_inclusive_tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let is_token = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '"';
    let mut start = None;
    for (index, c) in line.char_indices() {
        if is_token(c) {
            if start.is_none() {
                start = Some(index);
            }
        } else {
            if let Some(s) = start.take() {
                out.push((s, &line[s..index]));
            }
            out.push((index, &line[index..index + c.len_utf8()]));
        }
    }
    if let Some(s) = start {
        out.push((s, &line[s..]));
    }
    out
}

/// The base-type name of a literal TOKEN, when it is one: ints, floats,
/// bigints, bools, and double-quoted strings.
fn literal_base(token: &str) -> Option<&'static str> {
    if token == "true" || token == "false" {
        return Some("bool");
    }
    if token.starts_with('"') {
        return Some("string");
    }
    let numeric = token.strip_prefix('-').unwrap_or(token);
    if numeric.is_empty() || !numeric.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if let Some(mantissa) = numeric.strip_suffix('n') {
        return mantissa
            .chars()
            .all(|c| c.is_ascii_digit())
            .then_some("bigint");
    }
    if numeric.chars().all(|c| c.is_ascii_digit()) {
        return Some("int");
    }
    if numeric.chars().all(|c| c.is_ascii_digit() || c == '.')
        && numeric.chars().filter(|&c| c == '.').count() == 1
    {
        return Some("float");
    }
    None
}

/// Whether `hir` matches `tir` with every occurrence of `hole` treated
/// as a wildcard whose filled-in text satisfies `accept`.
fn wildcard_match(tir: &str, hole: &str, hir: &str, accept: &dyn Fn(&str) -> bool) -> bool {
    let segments: Vec<&str> = tir.split(hole).collect();
    let mut rest = hir;
    for (index, segment) in segments.iter().enumerate() {
        if index == 0 {
            let Some(after) = rest.strip_prefix(segment) else {
                return false;
            };
            rest = after;
            continue;
        }
        if index == segments.len() - 1 {
            let Some(filled) = rest
                .len()
                .checked_sub(segment.len())
                .map(|at| &rest[..at])
                .filter(|_| rest.ends_with(segment))
            else {
                return false;
            };
            return accept(filled);
        }
        let Some(found) = rest.find(segment) else {
            return false;
        };
        if !accept(&rest[..found]) {
            return false;
        }
        rest = &rest[found + segment.len()..];
    }
    true
}

/// TIR keeps an unresolved rigid effect var where hir_ty inferred the
/// effect: line-final ` throws X` with `X` a short capitalized
/// identifier on the TIR side.
fn throws_rigid_var_pair(tir: &str, hir: &str) -> bool {
    let (Some(t_split), Some(h_split)) = (tir.rfind(" throws "), hir.rfind(" throws ")) else {
        return false;
    };
    if tir[..t_split] != hir[..h_split] {
        return false;
    }
    let t_throws = tir[t_split + " throws ".len()..]
        .trim_end()
        .trim_end_matches(';');
    let h_throws = hir[h_split + " throws ".len()..]
        .trim_end()
        .trim_end_matches(';');
    // TIR keeps an unresolved rigid effect var where hir_ty inferred.
    if t_throws.len() <= 2
        && !t_throws.is_empty()
        && t_throws
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    // Effect PRECISION either direction (all ruled): hir's inferred set
    // inside TIR's declared context, or hir's written set where TIR
    // under-joined.
    let t_set: std::collections::BTreeSet<&str> = t_throws.split(" | ").map(str::trim).collect();
    let h_set: std::collections::BTreeSet<&str> = h_throws.split(" | ").map(str::trim).collect();
    t_set != h_set && (t_set.is_subset(&h_set) || h_set.is_subset(&t_set))
}

/// The first differing line pair, with one line of shared context above.
fn first_diff_excerpt(tir: &str, hir: &str) -> String {
    let tir_lines: Vec<&str> = tir.lines().collect();
    let hir_lines: Vec<&str> = hir.lines().collect();
    let common = tir_lines
        .iter()
        .zip(&hir_lines)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = String::new();
    if common > 0 {
        out.push_str(&format!("    ... {}\n", tir_lines[common - 1].trim_end()));
    }
    match (tir_lines.get(common), hir_lines.get(common)) {
        (Some(t), Some(h)) => {
            out.push_str(&format!("    tir: {}\n", t.trim_end()));
            out.push_str(&format!("    hir: {}\n", h.trim_end()));
        }
        (Some(t), None) => {
            out.push_str(&format!("    tir: {}\n", t.trim_end()));
            out.push_str("    hir: <ends>\n");
        }
        (None, Some(h)) => {
            out.push_str("    tir: <ends>\n");
            out.push_str(&format!("    hir: {}\n", h.trim_end()));
        }
        (None, None) => out.push_str("    <equal lines, trailing whitespace only>\n"),
    }
    out
}

#[test]
fn s16_provider_sweep_baml_src() {
    let root = baml_src_dir();
    let mut files = Vec::new();
    read_corpus_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!files.is_empty(), "no corpus files found");

    let mut db = crate::compiler2_tir::support::make_db();
    let loaded: Vec<(String, baml_base::SourceFile)> = files
        .into_iter()
        .map(|(rel, content)| {
            let file = db.add_file(&rel, &content);
            (rel, file)
        })
        .collect();

    let mut functions_compared = 0usize;
    let mut agreements = 0usize;
    let mut o2_agreements = 0usize;
    let mut ruled: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut differing: Vec<(String, String)> = Vec::new();
    let mut census: std::collections::BTreeMap<(String, String), (usize, String)> =
        std::collections::BTreeMap::new();
    let mut panics: Vec<String> = Vec::new();

    for (rel, file) in &loaded {
        let mut functions = file_functions(&db, *file).to_vec();
        functions.sort_by_key(|loc| function_source_map(&db, *loc).span.start());
        for func_loc in functions {
            let name = function_data(&db, func_loc).name.clone();
            let key = format!("{rel} :: {name}");
            functions_compared += 1;
            if let (Ok(tir), Ok(hir)) = (
                lower_pretty_at(&db, func_loc, InferenceProvider::Tir, OptLevel::Two),
                lower_pretty_at(&db, func_loc, InferenceProvider::HirTy, OptLevel::Two),
            ) && tir == hir
            {
                o2_agreements += 1;
            }
            let verdict = match (
                lower_pretty(&db, func_loc, InferenceProvider::Tir),
                lower_pretty(&db, func_loc, InferenceProvider::HirTy),
            ) {
                (Ok(tir), Ok(hir)) if tir == hir => Verdict::Agree,
                (Ok(tir), Ok(hir)) => match classify_diff(&tir, &hir) {
                    Some(buckets) => Verdict::Ruled { buckets },
                    None => {
                        for pair in residual_pairs(&tir, &hir) {
                            let entry = census.entry(pair).or_insert((0usize, String::new()));
                            entry.0 += 1;
                            if entry.1.is_empty() {
                                entry.1 = key.clone();
                            }
                        }
                        Verdict::Differ {
                            excerpt: first_diff_excerpt(&tir, &hir),
                        }
                    }
                },
                (Err(message), _) => Verdict::Panic {
                    side: "tir",
                    message,
                },
                (_, Err(message)) => Verdict::Panic {
                    side: "hir_ty",
                    message,
                },
            };
            match verdict {
                Verdict::Agree => agreements += 1,
                Verdict::Ruled { buckets } => *ruled.entry(buckets).or_insert(0usize) += 1,
                Verdict::Differ { excerpt } => differing.push((key, excerpt)),
                Verdict::Panic { side, message } => {
                    panics.push(format!("{key} [{side}] {message}"))
                }
            }
        }
    }

    let mut report = String::new();
    writeln!(report, "== S16 provider sweep ==").unwrap();
    writeln!(report, "functions compared: {functions_compared}").unwrap();
    writeln!(report, "agreements: {agreements}").unwrap();
    writeln!(
        report,
        "O2 agreements (informational; optimizer parity deferred): {o2_agreements}"
    )
    .unwrap();
    for (bucket, count) in &ruled {
        writeln!(report, "ruled {bucket}: {count}").unwrap();
    }
    writeln!(report, "differing: {}", differing.len()).unwrap();
    writeln!(report, "panics: {}", panics.len()).unwrap();
    writeln!(report, "top-level lets: not diffed (no body renderer yet)").unwrap();
    if !panics.is_empty() {
        writeln!(report, "\n== panics ==").unwrap();
        for line in &panics {
            writeln!(report, "{line}").unwrap();
        }
    }
    if !census.is_empty() {
        writeln!(
            report,
            "\n== residual pair census (all shapes, by count) =="
        )
        .unwrap();
        let mut ranked: Vec<(&(String, String), &(usize, String))> = census.iter().collect();
        ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
        for ((tir_line, hir_line), (count, example)) in ranked {
            writeln!(report, "[{count}x] e.g. {example}").unwrap();
            writeln!(report, "    tir: {tir_line}").unwrap();
            writeln!(report, "    hir: {hir_line}").unwrap();
        }
    }
    if !differing.is_empty() {
        writeln!(report, "\n== differing functions ==").unwrap();
        for (key, excerpt) in &differing {
            writeln!(report, "{key}").unwrap();
            write!(report, "{excerpt}").unwrap();
        }
    }

    assert_compiler2_snapshot!(super::SNAPSHOT_PATH, "s16_provider_sweep", report);
}
