//! S15: the pre-cutover differential sweep.
//!
//! Runs EVERY file of the living runtime corpus (`baml_tests/baml_src/`,
//! code written without hir_ty in mind) through both engines and
//! aggregates every node where they disagree into a classified report -
//! the DIVERGENCE LEDGER. Buckets:
//!
//! - `conflict`: both engines typed the node, differently. Grouped by the
//!   exact `(hir_ty, tir)` render pair with example sites - the raw
//!   material for classification (spec-mandated improvement / hir_ty bug
//!   / semantics ruling). Nothing here is "fixed" silently: the snapshot
//!   IS the review artifact.
//! - `one-sided`: only one engine recorded a type at the range (coverage
//!   differences, counted but not itemized).
//! - `hir_ty error channel`: mismatches/non-exhaustive entries on a
//!   corpus that TIR compiles clean - candidate engine bugs or
//!   stricter-by-spec verdicts, every entry itemized.
//! - `panic`: a file whose inference panicked in either engine -
//!   itemized; uncharted constructs are exactly what the sweep hunts.
//!
//! The exit criterion for cutover (S16): every conflict group is either
//! matched to a documented spec-ahead divergence or resolved; the ledger
//! then becomes the cutover changelog.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::harness::{
    NodeKind, collect_hir_ty_error_channel, collect_hir_ty_nodes, collect_tir_nodes,
    tir_error_diagnostics,
};

pub(crate) fn baml_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src")
}

pub(crate) fn read_corpus_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            read_corpus_files(root, &path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .expect("strip corpus prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path)
                .expect("read corpus file")
                .replace("\r\n", "\n");
            out.push((rel, content));
        }
    }
}

/// The ruled-divergence taxonomy, MACHINE-CHECKED: every conflict pair
/// must land in a bucket or the sweep fails. This is a coarse
/// accounting net for the S15 rulings, not an equivalence oracle - each
/// bucket corresponds to a documented ruling (crate README, S15/S15.5
/// sections); an unclassified pair means either a new regression or a
/// divergence nobody ruled on yet, and both must be looked at.
fn classify_divergence(hir: &str, tir: &str) -> Option<&'static str> {
    fn tokens(s: &str) -> Vec<&str> {
        s.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!' || c == '.'))
            .filter(|t| !t.is_empty())
            .collect()
    }
    fn sorted_tokens(s: &str) -> Vec<&str> {
        let mut t = tokens(s);
        t.sort_unstable();
        t
    }
    fn generic_var(token: &str) -> bool {
        // Single-letter frame vars, TIR's named placeholders, and its
        // un-defaulted effect params.
        if token == "Self" || token == "TFinal" || token.starts_with("__effect_param") {
            return true;
        }
        let mut chars = token.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
            && chars.as_str().chars().all(|c| c.is_ascii_digit())
            && token.len() <= 2
    }
    fn literal_base(token: &str) -> Option<&'static str> {
        if token == "true" || token == "false" {
            return Some("bool");
        }
        if token.ends_with('n') && token[..token.len() - 1].parse::<i128>().is_ok() {
            return Some("bigint");
        }
        if token.parse::<i64>().is_ok()
            || token.starts_with('-') && token[1..].parse::<i64>().is_ok()
        {
            return Some("int");
        }
        if token.parse::<f64>().is_ok() && token.contains('.') {
            return Some("float");
        }
        None
    }
    // Replace every quoted string literal with `string` (the raw
    // render keeps quotes; tokenization would strip them).
    fn fold_string_literals(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
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
    // Fold every literal token to its base primitive.
    fn widen_all(raw: &str) -> String {
        let s = fold_string_literals(raw);
        tokens(&s)
            .iter()
            .map(|t| literal_base(t).unwrap_or(t))
            .collect::<Vec<_>>()
            .join(" ")
    }
    // Strip namespace qualification (`baml.future.Future` -> `Future`,
    // `user.interfaces.X` -> `X`) and string-literal quotes.
    fn unqualified(s: &str) -> String {
        tokens(s)
            .iter()
            .map(|t| t.rsplit('.').next().unwrap_or(t))
            .collect::<Vec<_>>()
            .join(" ")
    }

    // 1. Self-showing callee renders (ruled: ours keeps the signature).
    if hir.contains("(self:") && !tir.contains("(self:") {
        return Some("self-showing-callee-render");
    }
    // 2. Throws precision (ruled: effect inference beats TIR's unknown).
    if tir.contains("throws unknown") && !hir.contains("throws unknown") {
        return Some("throws-precision");
    }
    // 3. TIR-behind uninstantiated generic vars / holes.
    {
        let hir_tokens = sorted_tokens(hir);
        let tir_has_var = tokens(tir)
            .iter()
            .any(|t| (generic_var(t) || *t == "unknown" || *t == "_") && !hir_tokens.contains(t));
        if tir_has_var {
            return Some("tir-uninstantiated");
        }
    }
    // 4. Rustc-strict unconstrained empty containers (ruled 2026-08-07).
    if hir.contains("!error")
        && (tir.contains("never[") || tir.contains("_[") || !tir.contains("!error"))
    {
        return Some("rustc-strict-empty-literal");
    }
    // 4b. TIR fails to type where we succeed (`!error`/`never` on the
    //     TIR side only) - the ours-ahead render families.
    if tir.contains("!error") && !hir.contains("!error") {
        return Some("tir-error");
    }
    if tir == "never" && hir != "never" {
        return Some("tir-never-render");
    }
    // 5. Pure arrangement differences: same token multiset (unions,
    //    pin order, null-last presentation).
    if sorted_tokens(hir) == sorted_tokens(tir) {
        return Some("canonical-order");
    }
    // 6. Ours-reduced: hir's members are a subset of tir's (C | I == I
    //    absorption, match-join canonicalization, `int` vs `int | 0`).
    {
        let h = sorted_tokens(hir);
        let t = sorted_tokens(tir);
        if h.iter().all(|x| t.contains(x)) && h.len() < t.len() {
            return Some("ours-reduced");
        }
        // 7. Ours-union-join: tir discards where we union (`int | void`
        //    vs `void`, the arm-join ruling).
        if t.iter().all(|x| h.contains(x)) && t.len() < h.len() {
            return Some("ours-union-join");
        }
    }
    // 8. Literal collapse / widening renders, either direction:
    //    `bool` vs `true | false`, `hir=[5] tir=[int]` (narrowed use),
    //    `Dog<int>` vs `Dog<42>` (generic-arg widening), thrown-literal
    //    unions vs their base. SET comparison (a widened literal union
    //    dedups); numeric literals fold across int/bigint/float bases
    //    when the other side is a single primitive.
    {
        fn token_set(s: &str) -> std::collections::BTreeSet<String> {
            s.split_whitespace().map(str::to_owned).collect()
        }
        let (wh, wt) = (widen_all(hir), widen_all(tir));
        if token_set(&wh) == token_set(&wt) {
            return Some("literal-collapse-render");
        }
        // All-literal side against one primitive base (bigint renders
        // its literals digit-only, so the base can differ).
        let primitives = ["int", "bigint", "float", "string", "bool"];
        let all_literal_vs_base = |side: &str, base: &str| {
            primitives.contains(&base)
                && tokens(&fold_string_literals(side))
                    .iter()
                    .all(|t| literal_base(t).is_some() || primitives.contains(t))
        };
        if all_literal_vs_base(hir, tir.trim()) || all_literal_vs_base(tir, hir.trim()) {
            return Some("literal-collapse-render");
        }
        // Widened subset, either direction (thrown-literal families
        // where one side also widened part of the union).
        let (sh, st) = (token_set(&wh), token_set(&wt));
        if sh.is_subset(&st) || st.is_subset(&sh) {
            return Some("literal-collapse-render");
        }
    }
    // 8b. Catch-residual precision: our fact subtraction proves an
    //     empty residual (`never`) where TIR keeps the thrown type.
    if hir == "never" && tir != "never" {
        return Some("ours-residual-never");
    }
    // 8c. Statement-position void joins (the int | void ruling family).
    {
        let voidish = |s: &str| matches!(s, "int" | "void" | "int | void");
        if voidish(hir) && voidish(tir) && hir != tir {
            return Some("ours-union-join");
        }
    }
    // 8d. TIR keeps a projection symbolic where we reduced it.
    if tir.contains(" as ") && !hir.contains(" as ") {
        return Some("tir-unreduced-projection");
    }
    // 8e. Complete enum variant sets collapse to the enum (ruled; the
    //     canonical algebra's complete-set rule).
    {
        let hir_single = !hir.contains(' ');
        let variants_of_hir = tir.split(" | ").all(|member| {
            member
                .strip_prefix(hir)
                .is_some_and(|rest| rest.starts_with('.'))
        });
        if hir_single && tir.contains(" | ") && variants_of_hir {
            return Some("enum-complete-collapse");
        }
    }
    // 8f. Alias-nominal and union-alias renders: ours keeps the written
    //     name where TIR expands (or vice versa). COARSE: both sides
    //     must be user-namespace nominals only; a genuinely wrong class
    //     would also match, so the per-key snapshot remains the review
    //     surface for this bucket.
    {
        let nominal_only = |s: &str| {
            !s.is_empty()
                && s.split(" | ")
                    .all(|member| member.starts_with("user.") && !member.contains(['(', '<']))
        };
        if nominal_only(hir) && nominal_only(tir) {
            return Some("alias-nominal");
        }
        // The written-ascription direction (ruling 3, recorded 2026-08-11:
        // pattern ascriptions record the WRITTEN form): hir renders the
        // written alias NAME where TIR expanded it into its union. Scoped
        // to a single bare name against a union render.
        let bare_name = |s: &str| {
            !s.is_empty()
                && !s.contains(' ')
                && !s.contains(['(', '<', '['])
                && s.chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
        };
        if bare_name(hir) && tir.contains(" | ") {
            return Some("written-ascription");
        }
    }
    // 8g. TIR's own type-error repro corpus (ns_type_error_repro): TIR
    //     renders its known-broken duplicated alias keys; ours resolves.
    if tir.contains("TypeErrReproStrKey") {
        return Some("tir-known-bug-repro");
    }
    // 8b. A written FUNCTION ascription: hir records the written type
    //     (ruling 3) - parameter labels absent, `throws unknown` as
    //     written - where TIR records the narrowed working form with
    //     labels and the inferred effect.
    {
        let strip_labels = |s: &str| -> String {
            let mut out = String::new();
            let mut chars = s.split_inclusive(|c: char| c == '(' || c == ',');
            for piece in chars.by_ref() {
                out.push_str(piece);
            }
            let _ = &mut out;
            let mut cleaned = String::new();
            for segment in out.split(&['(', ','][..]) {
                cleaned.push_str(segment.split_once(": ").map_or(segment, |(_, ty)| ty));
            }
            cleaned
        };
        if hir.contains("->")
            && tir.contains("->")
            && hir.contains("throws unknown")
            && strip_labels(&hir.replace("throws unknown", "throws _"))
                == strip_labels(
                    &tir[..]
                        .split(" throws ")
                        .next()
                        .map(|prefix| format!("{prefix} throws _"))
                        .unwrap_or_default(),
                )
        {
            return Some("written-ascription");
        }
    }

    // 9. Name-qualification and alias-nominal renders.
    if sorted_tokens(&unqualified(hir)) == sorted_tokens(&unqualified(tir)) {
        return Some("qualification-render");
    }
    if sorted_tokens(&unqualified(&widen_all(hir))) == sorted_tokens(&unqualified(&widen_all(tir)))
    {
        return Some("literal-collapse-render");
    }
    None
}

/// One divergence group: a distinct `(hir_ty, tir)` render pair.
#[derive(Default)]
struct ConflictGroup {
    count: usize,
    /// Up to two `file:start..end 'text'` example sites.
    examples: Vec<String>,
}

#[test]
fn s15_sweep_baml_src() {
    let root = baml_src_dir();
    let mut files = Vec::new();
    read_corpus_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!files.is_empty(), "no corpus files found");

    let mut db = crate::compiler2_tir::support::make_db();
    let loaded: Vec<(String, String, baml_base::SourceFile)> = files
        .into_iter()
        .map(|(rel, content)| {
            let file = db.add_file(&rel, &content);
            (rel, content, file)
        })
        .collect();

    let mut nodes_compared = 0usize;
    let mut agreements = 0usize;
    let mut one_sided = 0usize;
    let mut conflicts: BTreeMap<(String, String), ConflictGroup> = BTreeMap::new();
    let mut channel_entries: Vec<String> = Vec::new();
    let mut tir_diagnostics: Vec<String> = Vec::new();
    let mut panics: Vec<String> = Vec::new();

    for (rel, content, file) in &loaded {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let hir = collect_hir_ty_nodes(&db, *file, content);
            let tir = collect_tir_nodes(&db, *file, content);
            let channel = collect_hir_ty_error_channel(&db, *file);
            let diags = tir_error_diagnostics(&db, *file);
            (hir, tir, channel, diags)
        }));
        let (hir, tir, channel, diags) = match outcome {
            Ok(parts) => parts,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("<non-string panic>");
                panics.push(format!("{rel}: {}", first_line(msg)));
                continue;
            }
        };

        for diag in diags {
            tir_diagnostics.push(format!("{rel}: {}", first_line(&diag)));
        }
        for (&(start, end), rendered) in &channel.mismatches {
            for entry in rendered {
                channel_entries.push(format!(
                    "{rel}:{start}..{end} `{}`: {entry}",
                    snippet(content, start, end)
                ));
            }
        }
        for range in &channel.non_exhaustive {
            let (start, end) = (u32::from(range.start()), u32::from(range.end()));
            channel_entries.push(format!(
                "{rel}:{start}..{end} `{}`: non-exhaustive",
                snippet(content, start, end)
            ));
        }

        // Merge nodes per range, exactly the dump's discipline.
        let mut merged: BTreeMap<(u32, u32), (Vec<String>, Vec<String>)> = BTreeMap::new();
        for (nodes, side) in [(&hir, 0usize), (&tir, 1usize)] {
            for node in nodes.iter() {
                if node.kind == NodeKind::BindingName {
                    continue;
                }
                let entry = merged
                    .entry((u32::from(node.range.start()), u32::from(node.range.end())))
                    .or_default();
                let list = if side == 0 {
                    &mut entry.0
                } else {
                    &mut entry.1
                };
                if !list.contains(&node.ty) {
                    list.push(node.ty.clone());
                }
            }
        }
        for ((start, end), (mut h, mut t)) in merged {
            h.sort();
            t.sort();
            nodes_compared += 1;
            if h == t {
                agreements += 1;
            } else if h.is_empty() || t.is_empty() {
                one_sided += 1;
            } else {
                let group = conflicts.entry((h.join(" / "), t.join(" / "))).or_default();
                group.count += 1;
                if group.examples.len() < 2 {
                    group.examples.push(format!(
                        "{rel}:{start}..{end} `{}`",
                        snippet(content, start, end)
                    ));
                }
            }
        }
    }

    let mut report = String::new();
    use std::fmt::Write as _;
    let conflict_total: usize = conflicts.values().map(|group| group.count).sum();
    let _ = writeln!(report, "files: {}", loaded.len());
    let _ = writeln!(report, "nodes compared: {nodes_compared}");
    let _ = writeln!(report, "agreements: {agreements}");
    let _ = writeln!(report, "one-sided (coverage): {one_sided}");
    let _ = writeln!(
        report,
        "conflicts: {conflict_total} across {} distinct pairs",
        conflicts.len()
    );
    let _ = writeln!(
        report,
        "hir_ty error-channel entries: {}",
        channel_entries.len()
    );
    let _ = writeln!(report, "tir diagnostics: {}", tir_diagnostics.len());
    let _ = writeln!(report, "panics: {}", panics.len());

    if !panics.is_empty() {
        let _ = writeln!(report, "\n== panics ==");
        for line in &panics {
            let _ = writeln!(report, "{line}");
        }
    }
    if !tir_diagnostics.is_empty() {
        let _ = writeln!(report, "\n== tir diagnostics ==");
        for line in &tir_diagnostics {
            let _ = writeln!(report, "{line}");
        }
    }
    if !channel_entries.is_empty() {
        let _ = writeln!(report, "\n== hir_ty error channel ==");
        for line in &channel_entries {
            let _ = writeln!(report, "{line}");
        }
    }
    let mut buckets: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut unclassified: Vec<String> = Vec::new();
    for ((h, t), group) in &conflicts {
        match classify_divergence(h, t) {
            Some(bucket) => *buckets.entry(bucket).or_default() += group.count,
            None => unclassified.push(format!(
                "[{}x] hir_ty=[{h}] tir=[{t}]\n    at {}",
                group.count,
                group.examples.first().map(String::as_str).unwrap_or("?")
            )),
        }
    }
    let _ = writeln!(report, "\n== ruled divergence buckets ==");
    for (bucket, count) in &buckets {
        let _ = writeln!(report, "{count}x {bucket}");
    }
    assert!(
        unclassified.is_empty(),
        "sweep conflicts without a ruled classification (new regression or \
         missing ruling - see crate README S15/S15.5):\n{}",
        unclassified.join("\n")
    );
    if !conflicts.is_empty() {
        let _ = writeln!(report, "\n== conflicts by (hir_ty, tir) pair ==");
        let mut ordered: Vec<_> = conflicts.iter().collect();
        ordered.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        for ((h, t), group) in ordered {
            let _ = writeln!(report, "[{}x] hir_ty=[{h}] tir=[{t}]", group.count);
            for example in &group.examples {
                let _ = writeln!(report, "    at {example}");
            }
        }
    }

    insta::assert_snapshot!("s15_sweep_baml_src", report);
}

fn snippet(content: &str, start: u32, end: u32) -> String {
    let raw = &content[start as usize..end as usize];
    let flat = raw.replace('\n', "\\n");
    if flat.len() <= 24 {
        flat
    } else {
        format!("{}...{}", &flat[..10], &flat[flat.len() - 11..])
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}
