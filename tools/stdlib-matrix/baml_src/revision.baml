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
/// concluded about each TypeScript symbol.
class Revision {
    provenance: MatrixProvenance,
    /// Declaration text by id, both languages in one table — freshness asks the
    /// same question of both sides and ids do not collide across them.
    texts: map<string, string>,
    /// Which BAML symbols existed. Kept apart from `texts` because judging
    /// whether an *absence* still holds asks about the surface as a whole
    /// rather than about any one symbol — see `judgement_carries`.
    baml_ids: map<string, bool>,
    judgements: map<string, Judgement[]>,
}

/// What separates the fields of a comparison key.
///
/// A vertical tab, because it cannot occur in a docstring or a printed
/// signature and so keeps the key unambiguous — two symbols differing only in
/// where a field boundary falls must not compare equal.
///
/// Not `\u{1f}`, which is what this was: BAML does not support that escape, so
/// the separator was the six printable characters `\u{1f}` — ordinary text
/// that content can contain, defeating the whole point. `\v` and `\f` are the
/// control escapes the lexer does accept.
function field_separator() -> string {
    "\v"
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
        .join(field_separator())
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
    [symbol.origin, symbol.display, symbol.signature, symbol.doc ?? "", symbol.since].join(
        field_separator(),
    )
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
    if (report.format_version != 2) {
        throw baml.errors.InvalidArgument {
            message: `${path} has report format_version ${report.format_version} (expected 2); regenerate it with tools/stdlib-matrix/run`,
        };
    }

    let texts: map<string, string> = {};
    let baml_ids: map<string, bool> = {};
    for (let symbol in report.baml) {
        let _ = texts.set(symbol.id, prior_symbol_text(symbol));
        let _ = baml_ids.set(symbol.id, true);
    }
    for (let symbol in report.ts) {
        let _ = texts.set(symbol.id, ts_symbol_text(symbol));
    }
    let judgements: map<string, Judgement[]> = {};
    for (let judgement in report.judgements) {
        let bucket: Judgement[] = judgements.get(judgement.ts) ?? [];
        bucket.push(judgement);
        let _ = judgements.set(judgement.ts, bucket);
    }
    Revision {
        provenance: report.provenance,
        texts: texts,
        baml_ids: baml_ids,
        judgements: judgements,
    }
}

/// The revision a matrix would be, had it been written out and read back.
///
/// The round trip is what keeps `baml_symbol_text` and `prior_symbol_text`
/// honest: if the two ever disagreed, a surface would compare unchanged against
/// itself and never go stale.
function revision_of(matrix: SymbolMatrix) -> Revision {
    let judgements: map<string, Judgement[]> = {};
    for (let judgement in matrix.judgements) {
        let bucket: Judgement[] = judgements.get(judgement.ts) ?? [];
        bucket.push(judgement);
        let _ = judgements.set(judgement.ts, bucket);
    }
    let baml_ids: map<string, bool> = {};
    for (let symbol in matrix.baml) {
        let _ = baml_ids.set(symbol.id, true);
    }
    Revision {
        provenance: matrix.provenance,
        texts: current_texts(matrix),
        baml_ids: baml_ids,
        judgements: judgements,
    }
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
    /// TypeScript symbols whose judgement could not be carried and that a pass
    /// must therefore look at.
    queued: int,
}

/// Whether the BAML surface has gained anything since a revision.
///
/// Only additions matter here; a symbol that went away cannot make a judgement
/// that said "there is nothing" wrong.
function baml_surface_grew(matrix: SymbolMatrix, before: Revision) -> bool {
    let grew = false;
    for (let symbol in matrix.baml) {
        if (!before.baml_ids.has(symbol.id)) {
            grew = true;
        }
    }
    grew
}

/// Whether a judgement may be carried, given what has changed.
///
/// A pairing survives when every BAML symbol it names — and everything each of
/// those names in turn — reads as it did. A judged *absence* is a claim about
/// the whole BAML surface rather than about any one symbol — "nothing in BAML does this job" — so it survives only while that
/// surface could not have gained the thing it says is missing.
///
/// `baml_grew` is that test, and it is deliberately a test for *additions*
/// rather than for any change at all. Transposing the old rule literally would
/// key an absence on the surface's content hash, which is correct and useless:
/// we are the team writing this stdlib, so the hash moves on nearly every run
/// and every absence would expire on every run, which is the whole cost this
/// file exists to avoid. Only new surface can falsify "there is nothing"; a
/// docstring reworded cannot.
///
/// The gap that leaves is a symbol redefined in place — same id, different job
/// — which could falsify an absence without the id set moving. That is rare
/// enough to be worth the incrementality, and `--review-all` is the answer when
/// a release has done it.
function judgement_carries(
    judgement: Judgement,
    now: map<string, string>,
    before: Revision,
    baml_grew: bool,
    references: map<string, string[]>,
) -> bool {
    // The verdict, not the counterpart list. A `match` that named nothing is
    // still a pairing claim, and asking the absence question about it would
    // cache it against "did BAML gain anything" — a rule written for a
    // different claim entirely.
    if (!judgement_pairs(judgement) && judgement.verdict != "unnecessary") {
        return !baml_grew;
    }
    let carries = true;
    for (let id in judgement.baml) {
        // Freshness of the whole reachable set, not a bare text comparison: a
        // judgement is only as stable as what the symbols it names are built
        // from. This is the check the module header describes with
        // `read(path) -> baml.fs.DirEntry[]`, and it ran on the TypeScript
        // side alone — so retyping a field of `DirEntry` left every judgement
        // about reading a directory carried, shipping reasoning written
        // against the old type.
        if (!is_fresh_transitively(id, now, before, references)) {
            carries = false;
        }
    }
    carries
}

/// Whether `id`, and everything reachable from it, reads as it did.
///
/// Reachable rather than adjacent. The walk used to stop one step out, so a
/// judgement naming `A` carried even when `A` referenced `B` and `B` had been
/// built from a changed `C` — while the header above already promised
/// "everything each of those names in turn". The prose was right and the code
/// was one hop short.
///
/// Measured on the shipped surface the full closure is barely wider than the
/// neighbours — mean 3.0 symbols against 2.8, the same maximum of 17, and not
/// one symbol reaching even a tenth of the surface — because references point
/// at type declarations, which name nothing by existing. So the honest walk
/// costs almost nothing in judgements needlessly expired, which is the cost
/// this file exists to control.
///
/// Cycle-safe: `seen` gates every push, and a standard library is full of
/// mutually referential types.
function is_fresh_transitively(
    root: string,
    now: map<string, string>,
    before: Revision,
    references: map<string, string[]>,
) -> bool {
    let no_references: string[] = [];
    let seen: map<string, bool> = {};
    let pending: string[] = [root];
    while (pending.length() > 0) {
        let id = match (pending.pop()) {
            null => {
                break;
            },
            let popped: string => popped,
        };
        if (seen.has(id)) {
            continue;
        }
        let _ = seen.set(id, true);
        if (!before.texts.has(id) || before.texts.get(id) != now.get(id)) {
            return false;
        }
        for (let reference in references.get(id) ?? no_references) {
            if (!seen.has(reference)) {
                pending.push(reference);
            }
        }
    }
    true
}

/// Every BAML symbol's references, by id — what `judgement_carries` needs in
/// order to look one step past the symbols a judgement names.
function baml_references(matrix: SymbolMatrix) -> map<string, string[]> {
    let references: map<string, string[]> = {};
    for (let symbol in matrix.baml) {
        let _ = references.set(symbol.id, symbol.references);
    }
    references
}

/// Folds a previous revision into a fresh matrix.
///
/// A fresh matrix proposes nothing, so this does not have to choose between a
/// carried judgement and a competing one — it fills in what a previous run
/// concluded and leaves everything else unjudged for the passes.
function apply_revision(matrix: SymbolMatrix, before: Revision) -> CarryOutcome {
    let now = current_texts(matrix);
    let grew = baml_surface_grew(matrix, before);
    let references = baml_references(matrix);
    let carried: Judgement[] = [];
    let carried_for: map<string, bool> = {};
    let queued = 0;

    for (let symbol in matrix.ts) {
        let prior: Judgement[] = before.judgements.get(symbol.id) ?? [];
        if (prior.length() == 0 || !is_fresh(symbol.id, symbol.references, now, before)) {
            queued += 1;
            continue;
        }
        let all_carry = true;
        for (let judgement in prior) {
            if (!judgement_carries(judgement, now, before, grew, references)) {
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
        if (!carried_for.has(judgement.ts)) {
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
