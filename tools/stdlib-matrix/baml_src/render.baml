// Markdown view of the matrix — a readable companion to the canonical JSON.
//
// The JSON serves both languages' hierarchies from one relation; markdown has
// no view toggle, so this renders the BAML-rooted view and lists the
// TypeScript symbols nothing claimed at the end. A reader who wants the
// TypeScript-rooted view has the web page.
//
// Keep the output deterministic: no timestamps, no environment.

function escape_cell(value: string) -> string {
    value
        .replace_all("\\", "\\\\")
        .replace_all("|", "\\|")
        .replace_all("\r", "")
        .replace_all("\n", "<br>")
}

/// Stands in for a version the extractor could not determine. Shared with the
/// ratchet so an absent version reads the same when reported as when rendered.
function provenance_absent() -> string {
    "absent"
}

/// Shows the reader which inputs this view was built from. Purely
/// presentational: `run --check` reads provenance from the canonical JSON, so
/// this block is free to change shape without breaking the ratchet.
function render_inputs_section(provenance: MatrixProvenance) -> string {
    [
        `- BAML stdlib surface: \`${provenance.baml_surface_sha256}\` (sha256 of the export)`,
        `- BAML export format: v${provenance.export_format_version}`,
        `- TypeScript: ${provenance.typescript_version}`,
        `- @types/node: ${provenance.types_node_version ?? provenance_absent()}`,
    ]
    .join("\n")
}

/// The group a BAML symbol belongs to: its owner path, or the impl it arrives
/// through. Mirrors `groupOf` in the web view.
function baml_group_of(symbol: BamlSymbol) -> string {
    let names: string[] = [];
    let impl_group = "";
    for (let step in symbol.symbol) {
        match (step) {
            let plain: string => {
                names.push(plain);
            },
            let node: ImplStep => {
                impl_group = `${node.base} as ${node.iface.join(".")}`;
            },
        }
    }
    if (impl_group.length() > 0) {
        impl_group
    } else if (names.length() > 1) {
        names.slice(0, names.length() - 1).join(".")
    } else {
        names.at(0) ?? "(root)"
    }
}

/// One row: the symbol, what was concluded about it, and on what grounds.
///
/// The state column distinguishes the three things a reader needs apart: a
/// counterpart, a considered absence, and a symbol nothing has looked at yet.
function render_symbol_row(
    symbol: BamlSymbol,
    judgements: Judgement[],
    ts_by_id: map<string, TsSymbol>,
) -> string {
    let counterparts: string[] = [];
    let state = "unjudged";
    for (let judgement in judgements) {
        if (judgement_pairs(judgement)) {
            state
                = if (judgement.verdict == "divergent") {
                    "divergent"
                } else {
                    "both"
                };
            let note = if (judgement.basis == "model") {
                ` (${judgement.confidence ?? "?"})`
            } else {
                ""
            };
            let display = if let ts: TsSymbol = ts_by_id.get(judgement.ts ?? "") {
                ts.display
            } else {
                judgement.ts ?? ""
            };
            counterparts.push(`${display}${note}`);
        } else if (state == "unjudged") {
            state = "BAML only";
        }
    }
    [
        `| ${escape_cell(symbol.display)} `,
        `| ${escape_cell(symbol.signature?.display ?? "")} `,
        `| ${state} `,
        `| ${escape_cell(counterparts.join(", "))} |`,
    ]
    .join("")
}

function render_matrix_markdown(matrix: SymbolMatrix) -> string {
    let counts = matrix.counts;
    let ts_matched = counts.ts_symbols - counts.ts_unclaimed;

    // Judgements indexed by the symbol they are about, mirroring the web view.
    let by_baml: map<string, Judgement[]> = {};
    let ts_claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let bucket: Judgement[] = by_baml.get(judgement.baml) ?? [];
        bucket.push(judgement);
        let _ = by_baml.set(judgement.baml, bucket);
        if (judgement_pairs(judgement)) {
            if let ts: string = judgement.ts {
                let _ = ts_claimed.set(ts, true);
            }
        }
    }
    let ts_by_id: map<string, TsSymbol> = {};
    for (let symbol in matrix.ts) {
        let _ = ts_by_id.set(symbol.id, symbol);
    }

    let groups: map<string, BamlSymbol[]> = {};
    let order: string[] = [];
    for (let symbol in matrix.baml) {
        let group = baml_group_of(symbol);
        if (!groups.has(group)) {
            let empty: BamlSymbol[] = [];
            let _ = groups.set(group, empty);
            order.push(group);
        }
        if let bucket: BamlSymbol[] = groups.get(group) {
            bucket.push(symbol);
        }
    }
    order.sort_by_key((name) -> {
        name
    });

    let out: string[] = [];
    out.push("# BAML ↔ TypeScript stdlib matrix");
    out.push("");
    out.push("Generated by `tools/stdlib-matrix/run`. Do not edit by hand.");
    out.push("");
    out.push("## Inputs");
    out.push("");
    out.push(render_inputs_section(matrix.provenance));
    out.push("");
    out.push("## Summary");
    out.push("");
    out.push(
        `${counts.matched} of ${counts.baml_symbols} BAML symbols have a TypeScript counterpart · ${counts.unmatched} were judged to have none · ${counts.unjudged} are still unjudged.`,
    );
    out.push("");
    out.push(
        `${ts_matched} of ${counts.ts_symbols} TypeScript symbols are claimed by a BAML symbol · ${counts.ts_unclaimed} are not.`,
    );
    out.push("");
    out.push(
        "A counterpart rests either on deterministic name agreement or on a model's judgment, which carries its confidence in parentheses. `divergent` marks the same operation reached differently.",
    );
    out.push("");
    out.push("## BAML surface");
    out.push("");
    for (let group in order) {
        out.push(`### ${group}`);
        out.push("");
        out.push("| symbol | signature | state | TypeScript |");
        out.push("| --- | --- | --- | --- |");
        let members: BamlSymbol[] = groups.get(group) ?? [];
        for (let member in members) {
            let judgements: Judgement[] = by_baml.get(member.id) ?? [];
            out.push(render_symbol_row(member, judgements, ts_by_id));
        }
        out.push("");
    }

    out.push("## TypeScript symbols with no BAML counterpart");
    out.push("");
    out.push("| symbol | signature | since |");
    out.push("| --- | --- | --- |");
    for (let symbol in matrix.ts) {
        if (!ts_claimed.has(symbol.id)) {
            out.push(
                `| ${escape_cell(symbol.display)} | ${escape_cell(symbol.signature)} | ${symbol.since} |`,
            );
        }
    }
    out.push("");
    if (matrix.failures.length() > 0) {
        out.push("## Calls that did not come back");
        out.push("");
        out.push("| pass | subject | reason |");
        out.push("| --- | --- | --- |");
        for (let failure in matrix.failures) {
            out.push(
                `| ${failure.pass} | ${escape_cell(failure.subject)} | ${escape_cell(failure.reason)} |`,
            );
        }
        out.push("");
    }
    out.join("\n")
}
