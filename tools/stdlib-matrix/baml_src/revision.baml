// Carrying judgements forward from a previous revision.
//
// Most runs change almost nothing: a docstring reworded, one function added.
// Re-judging the whole surface each time is both expensive and unstable — the
// model has no temperature to pin, so two runs over identical inputs disagree.
// A previous report fixes both: a symbol nothing has touched keeps the answer
// it already has, and the run costs only what actually moved.
//
// What "nothing has touched" means is deliberately strict. A symbol's own text
// being identical is not enough, because its meaning is only as stable as the
// types it names: `read(path: string) -> baml.fs.DirEntry[]` means something
// new the moment `DirEntry` gains a field. So a judgement carries only when the
// symbol *and every symbol it names* are unchanged — which is what
// references.baml records.
//
// A carried judgement is not marked as carried. It is the same judgement, made
// on the same evidence, and stamping the run that copied it would say something
// about this invocation rather than about the two libraries. The count is
// reported so a reader knows how much was skipped, and `--review-all` forces
// the examination anyway.

/// A previous report, read for what it concluded.
///
/// Reduced on purpose: `BamlSymbol.symbol` is a `string | ImplStep` union, and
/// union decoding does not dispatch to a member's `FromJson`, so a full
/// `SymbolMatrix` cannot be read back (see the note in ingest.baml). Nothing
/// here needs the path — identity is the id — so the field is simply not
/// declared, and decoding ignores it.
class PriorSymbol {
    id: string,
    kind: string,
    origin: string,
    display: string,
    generics: string[],
    ty: string?,
    resolved: string?,
    default: string?,
    signature: SymbolSignature?,
    doc: string?,
    references: string[],
}

class PriorReport {
    format_version: int,
    provenance: MatrixProvenance,
    baml: PriorSymbol[],
    ts: TsSymbol[],
    judgements: Judgement[],
}

/// What a previous revision knew: every symbol's declaration text, and what was
/// concluded about each BAML symbol.
class Revision {
    provenance: MatrixProvenance,
    /// Declaration text by id, both languages in one table — freshness asks the
    /// same question of both sides and ids do not collide across them.
    texts: map<string, string>,
    judgements: map<string, Judgement[]>,
}

/// The text a declaration is compared by.
///
/// Everything that could change what a symbol *is*, and nothing that could not:
/// no ids (identity, not content), no references (derived from this text), no
/// paths (a rename shows up in the display). Assembled from parts rather than
/// from a serialization so that a change to the report's shape does not read as
/// a change to every symbol in it.
function declaration_text(
    kind: string,
    display: string,
    generics: string[],
    ty: string?,
    resolved: string?,
    fallback: string?,
    signature: string,
    doc: string?,
) -> string {
    [
        kind,
        display,
        generics.join(","),
        ty ?? "",
        resolved ?? "",
        fallback ?? "",
        signature,
        doc ?? "",
    ]
    .join("\u{1f}")
}

function baml_symbol_text(symbol: BamlSymbol) -> string {
    declaration_text(
        symbol.kind,
        symbol.display,
        symbol.generics,
        symbol.ty,
        symbol.resolved,
        symbol.default,
        symbol.signature?.display ?? "",
        symbol.doc,
    )
}

/// The same fields in the same order as `baml_symbol_text`. The two are
/// adjacent so that a field added to one and forgotten in the other is visible
/// here rather than as a surface that never goes stale.
function prior_symbol_text(symbol: PriorSymbol) -> string {
    declaration_text(
        symbol.kind,
        symbol.display,
        symbol.generics,
        symbol.ty,
        symbol.resolved,
        symbol.default,
        symbol.signature?.display ?? "",
        symbol.doc,
    )
}

/// TypeScript declares no error contract and its signature is printed text, so
/// there is less to compare — but the shape of the question is the same.
function ts_symbol_text(symbol: TsSymbol) -> string {
    [symbol.origin, symbol.display, symbol.signature, symbol.doc ?? "", symbol.since].join("\u{1f}")
}

/// Reads a previous report. A file that cannot be decoded is an error, never a
/// silent "no previous revision": treating an unreadable report as absent would
/// quietly turn an incremental run into a full one, which is the failure mode
/// this exists to prevent.
function load_revision(path: string) -> Revision {
    let report = baml.json.to<PriorReport>(baml.json.parse(baml.fs.read(path))) catch (_) {
        baml.json.JsonParseError => {
            throw baml.errors.InvalidArgument {
                message: `${path} is not valid JSON; regenerate it with tools/stdlib-matrix/run`,
            }
        },
        baml.json.JsonDecodeError => {
            throw baml.errors.InvalidArgument {
                message: `${path} does not match the current report model, probably written by an older tool; regenerate it with tools/stdlib-matrix/run`,
            }
        },
    };
    if (report.format_version != 1) {
        throw baml.errors.InvalidArgument {
            message: `${path} has report format_version ${report.format_version} (expected 1); regenerate it with tools/stdlib-matrix/run`,
        };
    }

    let texts: map<string, string> = {};
    for (let symbol in report.baml) {
        let _ = texts.set(symbol.id, prior_symbol_text(symbol));
    }
    for (let symbol in report.ts) {
        let _ = texts.set(symbol.id, ts_symbol_text(symbol));
    }
    let judgements: map<string, Judgement[]> = {};
    for (let judgement in report.judgements) {
        let bucket: Judgement[] = judgements.get(judgement.baml) ?? [];
        bucket.push(judgement);
        let _ = judgements.set(judgement.baml, bucket);
    }
    Revision { provenance: report.provenance, texts: texts, judgements: judgements }
}

/// The revision a matrix would be, had it been written out and read back.
///
/// The round trip is what keeps `baml_symbol_text` and `prior_symbol_text`
/// honest: if the two ever disagreed, a surface would compare unchanged against
/// itself and never go stale.
function revision_of(matrix: SymbolMatrix) -> Revision {
    let judgements: map<string, Judgement[]> = {};
    for (let judgement in matrix.judgements) {
        let bucket: Judgement[] = judgements.get(judgement.baml) ?? [];
        bucket.push(judgement);
        let _ = judgements.set(judgement.baml, bucket);
    }
    Revision { provenance: matrix.provenance, texts: current_texts(matrix), judgements: judgements }
}

/// Every current symbol's declaration text, in one table keyed the same way the
/// revision's is.
function current_texts(matrix: SymbolMatrix) -> map<string, string> {
    let texts: map<string, string> = {};
    for (let symbol in matrix.baml) {
        let _ = texts.set(symbol.id, baml_symbol_text(symbol));
    }
    for (let symbol in matrix.ts) {
        let _ = texts.set(symbol.id, ts_symbol_text(symbol));
    }
    texts
}

/// Whether a symbol reads exactly as it did, itself and everything it names.
///
/// A symbol absent from the previous revision is not fresh — it is new, and
/// nothing has judged it. So is one whose reference resolves to a symbol the
/// previous revision did not have.
function is_fresh(
    id: string,
    references: string[],
    now: map<string, string>,
    before: Revision,
) -> bool {
    let fresh = before.texts.get(id) == now.get(id) && before.texts.has(id);
    for (let reference in references) {
        if (!fresh) {
            continue;
        }
        fresh = before.texts.has(reference) && before.texts.get(reference) == now.get(reference);
    }
    fresh
}

/// What carrying a revision forward achieved, and what is left to do.
class CarryOutcome {
    matrix: SymbolMatrix,
    /// Judgements taken from the previous revision without asking again.
    carried: int,
    /// BAML symbols whose judgement could not be carried and that a pass must
    /// therefore look at.
    queued: int,
}

/// Whether a judgement may be carried, given what has changed.
///
/// A pairing survives when both its ends read as they did. A judged *absence*
/// is a claim about the whole other surface rather than about one symbol, so it
/// survives only while that surface is the one it was made against — a
/// TypeScript release could have added the very thing it says does not exist.
function judgement_carries(
    judgement: Judgement,
    now: map<string, string>,
    before: Revision,
    provenance: MatrixProvenance,
) -> bool {
    match (judgement.ts) {
        let ts: string => {
            before.texts.has(ts) && before.texts.get(ts) == now.get(ts)
        },
        null => {
            provenance.typescript_version == before.provenance.typescript_version
                && (provenance.types_node_version ?? provenance_absent())
                    == (before.provenance.types_node_version ?? provenance_absent())
        },
    }
}

/// Folds a previous revision into a fresh matrix.
///
/// A carried judgement replaces whatever the deterministic pass proposed for
/// that symbol, because it is the more considered answer: a name match is a
/// heuristic, and a judgement that survived a previous run's scrutiny outranks
/// one minted seconds ago. Symbols that are not fresh keep their proposals and
/// are left for the passes.
function apply_revision(matrix: SymbolMatrix, before: Revision) -> CarryOutcome {
    let now = current_texts(matrix);
    let carried: Judgement[] = [];
    let carried_for: map<string, bool> = {};
    let queued = 0;

    for (let symbol in matrix.baml) {
        let prior: Judgement[] = before.judgements.get(symbol.id) ?? [];
        if (prior.length() == 0 || !is_fresh(symbol.id, symbol.references, now, before)) {
            queued += 1;
            continue;
        }
        let all_carry = true;
        for (let judgement in prior) {
            if (!judgement_carries(judgement, now, before, matrix.provenance)) {
                all_carry = false;
            }
        }
        if (!all_carry) {
            queued += 1;
            continue;
        }
        let _ = carried_for.set(symbol.id, true);
        for (let judgement in prior) {
            carried.push(judgement);
        }
    }

    let kept: Judgement[] = [];
    for (let judgement in matrix.judgements) {
        if (!carried_for.has(judgement.baml)) {
            kept.push(judgement);
        }
    }
    for (let judgement in carried) {
        kept.push(judgement);
    }
    matrix.judgements = kept;
    matrix.counts = recount(matrix);
    CarryOutcome { matrix: matrix, carried: carried.length(), queued: queued }
}
