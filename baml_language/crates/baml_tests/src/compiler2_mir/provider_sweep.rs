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

use baml_compiler2_mir::{InferenceProvider, OptLevel, lower_function_with, pretty::display_function};
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
    Ruled { buckets: String },
    Differ { excerpt: String },
    Panic { side: &'static str, message: String },
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
    let mut flush = |deletes: &mut Vec<String>, inserts: &mut Vec<String>, events: &mut Vec<AlignEvent>| {
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

fn classify_diff(tir: &str, hir: &str) -> Option<String> {
    let mut buckets: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    let mut renumber_only = false;
    for event in aligned_events(tir, hir) {
        match event {
            AlignEvent::Pair(tl, hl) => match classify_pair(&tl, &hl)? {
                Some(bucket) => {
                    buckets.insert(bucket);
                }
                None => renumber_only = true,
            },
            AlignEvent::HirOnly(hl) => {
                if !seeding_insertion(&hl) {
                    return None;
                }
                buckets.insert("receiver-seeding");
            }
            AlignEvent::TirOnly(_) => return None,
        }
    }
    // A pure-renumbering pair is only admissible as the CONSEQUENCE of
    // an accepted insertion; renumbering with no cause stays itemized.
    if renumber_only && !buckets.contains("receiver-seeding") {
        return None;
    }
    (!buckets.is_empty()).then(|| {
        buckets
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join("+")
    })
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
    let t_norm = normalize_union_payloads(&dedup_union_members(&collapse_json_alias(
        &normalize_type_literals(&t_ids),
    )));
    let h_norm = normalize_union_payloads(&dedup_union_members(&collapse_json_alias(
        &normalize_type_literals(&h_ids),
    )));
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
    if let (Some(t_at), Some(h_at)) = (t_norm.find("is_type(copy _, "), h_norm.find("is_type(copy _, "))
        && t_norm[..t_at] == h_norm[..h_at]
        && t_norm[t_at..].trim_end() == "is_type(copy _, never);"
        && h_norm[h_at..].trim_end().ends_with(");")
    {
        return Some(Some("tir-never-test"));
    }
    if throws_rigid_var_pair(&t_norm, &h_norm) {
        return Some(Some("throws-precision"));
    }
    None
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
        if t_ty.starts_with(base) && h_ty.starts_with(&format!("Literal({base}(")) {
            return true;
        }
    }
    false
}

/// Replaces every local id (`_12`) with `_` so renumbering compares
/// equal; ids appear only in this shape in the pretty-printer.
fn strip_local_ids(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '_' {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
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

/// Canonicalizes the union payload of `load_type(...)`/`is_type(_, ...)`
/// operands: members SORT (the S15 canonical-order ruling - TIR renders
/// written order) and an enum's variant member is ABSORBED by the enum
/// itself (`E | E.V` is `E`, the S15 canonical-union absorption ruling).
/// Depth-aware split so generics carrying `|` inside `<>` stay intact.
fn normalize_union_payloads(line: &str) -> String {
    let mut out = line.to_string();
    for opener in ["load_type(", "is_type(copy _, "] {
        let Some(start) = out.find(opener) else { continue };
        let payload_start = start + opener.len();
        let Some(rel_end) = out[payload_start..].find(')') else { continue };
        let payload = &out[payload_start..payload_start + rel_end];
        let mut members: Vec<&str> = Vec::new();
        let (mut depth, mut last) = (0usize, 0usize);
        for (index, c) in payload.char_indices() {
            match c {
                '<' | '[' => depth += 1,
                '>' | ']' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => {
                    members.push(payload[last..index].trim());
                    last = index + 1;
                }
                _ => {}
            }
        }
        members.push(payload[last..].trim());
        if members.len() < 2 {
            continue;
        }
        let absorbed: Vec<&str> = members
            .iter()
            .copied()
            .filter(|member| {
                !members
                    .iter()
                    .any(|other| member.strip_prefix(other).is_some_and(|rest| rest.starts_with('.')))
            })
            .collect();
        let mut sorted = absorbed;
        sorted.sort_unstable();
        sorted.dedup();
        let canonical = sorted.join(" | ");
        out = format!(
            "{}{}{}",
            &out[..payload_start],
            canonical,
            &out[payload_start + rel_end..]
        );
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
    let t_throws = tir[t_split + " throws ".len()..].trim_end();
    t_throws.len() <= 2
        && !t_throws.is_empty()
        && t_throws
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
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
        writeln!(report, "\n== residual pair census (all shapes, by count) ==").unwrap();
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
