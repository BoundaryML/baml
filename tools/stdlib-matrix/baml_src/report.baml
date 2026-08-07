// The matrix: two symbol lists and the correspondences between them.
//
// Both sides are stored flat, each symbol carrying its own path, with
// judgements as a separate relation. That much is symmetric, and both views
// read the same relation — a TypeScript-rooted view groups `ts` by module, a
// BAML-rooted view groups `baml` by namespace, and nothing is duplicated.
//
// The relation itself is not symmetric, and deliberately so. A judgement is
// keyed on a TypeScript symbol because that is the question the matrix answers:
// a reader knows how to do something in TypeScript and wants to know how it is
// done in BAML. The BAML view is a reverse lookup over the same judgements.
//
// The consequence is worth stating plainly, because it is easy to misread the
// numbers otherwise. A TypeScript symbol can be judged to have no counterpart —
// something looked, and that absence is a finding. A BAML symbol that no
// judgement names is *not* the same claim: nothing asked whether it has a
// counterpart, so it is silence rather than a finding. `unmatched` and
// `baml_unclaimed` count those two different things and must not be added up.

/// Two snippets showing the same job done in each language.
///
/// Only worth carrying where the shapes genuinely differ — an exact match
/// teaches nothing by being spelled twice. A verifier that has just worked out
/// *how* two APIs differ is the one place in the pipeline that knows enough to
/// write this, which is why it is attached to the judgement rather than
/// generated later.
class CodeExample {
    typescript: string,
    baml: string,
    note: string?,
}

/// What was concluded about one TypeScript symbol, and on what grounds.
///
/// Keyed on the TypeScript side, because that is the side a reader arrives
/// from: they know how to do something in TypeScript and want to know how it is
/// done in BAML. A judgement is the answer to that question, so the question is
/// the key.
///
/// `baml` is a list. "How you would do this" is not always one symbol —
/// `child_process.spawn` is `baml.sys.exec` plus its `ShellOutput`, and
/// `Array.prototype.sort` reaches BAML through both `sort` and the `Comparable`
/// interface it needs. Zero entries means nothing does this job.
///
/// Recorded rather than derived, because the interesting part is not the
/// pairing but the reasoning: a symbol judged to have no BAML counterpart is a
/// finding about the two libraries, and one judged to have a *divergent*
/// counterpart — the same operation reached differently — is the finding this
/// matrix exists for. Both are indistinguishable from silence unless written
/// down.
///
/// Every side is an id, never an array index. Indices shift whenever either
/// stdlib does, which makes a report undiffable and a previous revision useless
/// as a cache; ids are content-derived and survive.
///
/// `verdict` is what is true:
///   - `match`        — the same operation, reached the same way
///   - `divergent`    — the same operation, reached differently; `divergence` says how
///   - `unnecessary`  — BAML makes the question not arise
///   - `none`         — nothing in BAML does this symbol's job
///
/// `unnecessary` is a distinct finding from `none`, and the more useful one. A
/// reader who cannot find `Array.isArray` needs to be told not that BAML lacks
/// it but that BAML does not need it: types are checked by pattern matching, so
/// the question only comes up for a value in a union, and then `foo is int[]`
/// answers it. That is a different thing to say than "there is nothing here",
/// and rendering the two the same way loses the whole explanation.
///
/// Such a judgement may still name BAML symbols, and usually carries an
/// `example`: the answer is often syntax rather than a symbol, and `baml` can
/// only hold symbols.
///
/// `basis` is why it is believed:
///   - `name`           — deterministic agreement of name and owner
///   - `model`          — a model judged it, and said why
///   - `no-candidates`  — no plausible counterpart existed to ask about
///
/// There is no `carried` basis. A judgement held over from a previous revision
/// is the same judgement on the same evidence — see revision.baml — and marking
/// it would describe the run that copied it rather than the two libraries.
class Judgement {
    ts: string,
    baml: string[],
    verdict: string,
    basis: string,
    confidence: string?,
    reason: string?,
    /// How the two differ, when they are the same operation reached
    /// differently: a static on a primitive against a free function, an
    /// operator interface against built-in syntax.
    divergence: string?,
    /// Counterparts a judge considered and refused. Kept so a later run does
    /// not propose them again — a refutation is as much a result as a match.
    rejected: string[],
    /// Whether a judge examined this pairing, as opposed to it standing on the
    /// proposer's word alone.
    verified: bool,
    example: CodeExample?,
}

/// A call that did not come back. Recorded rather than swallowed: a failed
/// batch degrading to an empty verdict is indistinguishable from a batch that
/// honestly found nothing, and that difference is the whole question when a run
/// comes back thinner than the last.
class PassFailure {
    pass: string,
    subject: string,
    reason: string,
}

class MatrixCounts {
    baml_symbols: int,
    ts_symbols: int,
    judgements: int,
    /// TypeScript symbols with a BAML counterpart, exact or divergent.
    matched: int,
    /// TypeScript symbols BAML makes unnecessary. Counted apart from both
    /// `matched` and `unmatched`: it is neither a correspondence nor a gap.
    unnecessary: int,
    /// TypeScript symbols judged to have none — considered, and found wanting.
    unmatched: int,
    /// TypeScript symbols nothing has judged yet.
    unjudged: int,
    /// BAML symbols no judgement names. Not the same claim as the TypeScript
    /// side's `unmatched`: nothing asked whether these have a counterpart, so
    /// this counts silence, not a finding.
    baml_unclaimed: int,
}

class SymbolMatrix {
    format_version: int,
    provenance: MatrixProvenance,
    baml: BamlSymbol[],
    ts: TsSymbol[],
    judgements: Judgement[],
    failures: PassFailure[],
    counts: MatrixCounts,
}

/// A judgement that pairs this symbol with something, as opposed to one that
/// records an absence.
function judgement_pairs(judgement: Judgement) -> bool {
    judgement.verdict == "match" || judgement.verdict == "divergent"
}

/// The strongest thing said about a symbol, when more than one judgement speaks
/// for it.
///
/// Divergence outranks an exact match because it is the finding a reader is
/// scanning for, and `unnecessary` outranks an absence because it explains one.
/// An unrecognized verdict — and the absence of any — ranks below every real
/// one, so a symbol nothing has judged never displaces a symbol something has.
/// The view applies the same order, so the square and the tally cannot disagree.
function judgement_rank(verdict: string) -> int {
    match (verdict) {
        "divergent" => 4,
        "match" => 3,
        "unnecessary" => 2,
        "none" => 1,
        _ => 0,
    }
}

/// What a symbol's judgements amount to, or `""` when there are none.
function verdict_of(judgements: Judgement[]) -> string {
    let strongest = "";
    for (let judgement in judgements) {
        if (judgement_rank(judgement.verdict) > judgement_rank(strongest)) {
            strongest = judgement.verdict;
        }
    }
    strongest
}

/// Recomputes the tallies from the relation rather than by adjusting the old
/// numbers, so a pass that forgets to increment cannot leave the report lying.
function recount(matrix: SymbolMatrix) -> MatrixCounts {
    let verdicts: map<string, string> = {};
    let baml_claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let strongest = verdicts.get(judgement.ts) ?? "";
        if (judgement_rank(judgement.verdict) > judgement_rank(strongest)) {
            let _ = verdicts.set(judgement.ts, judgement.verdict);
        }
        // Named is named, whatever the verdict: a symbol offered as the way to
        // do something BAML makes unnecessary has still been pointed at.
        for (let id in judgement.baml) {
            let _ = baml_claimed.set(id, true);
        }
    }
    let matched = 0;
    let unnecessary = 0;
    let unmatched = 0;
    let unjudged = 0;
    for (let symbol in matrix.ts) {
        match (verdicts.get(symbol.id) ?? "") {
            "match" => {
                matched += 1;
            },
            "divergent" => {
                matched += 1;
            },
            "unnecessary" => {
                unnecessary += 1;
            },
            "none" => {
                unmatched += 1;
            },
            _ => {
                unjudged += 1;
            },
        }
    }
    let baml_unclaimed = 0;
    for (let symbol in matrix.baml) {
        if (!baml_claimed.has(symbol.id)) {
            baml_unclaimed += 1;
        }
    }
    MatrixCounts {
        baml_symbols: matrix.baml.length(),
        ts_symbols: matrix.ts.length(),
        judgements: matrix.judgements.length(),
        matched: matched,
        unnecessary: unnecessary,
        unmatched: unmatched,
        unjudged: unjudged,
        baml_unclaimed: baml_unclaimed,
    }
}

/// A TS symbol's own name — the last path step.
function ts_name_of(symbol: TsSymbol) -> string {
    symbol.symbol.at(symbol.symbol.length() - 1) ?? symbol.display
}

/// The owner two symbols must share for a name match to mean anything.
///
/// Matching on the member name alone is far too loose: `baml.env.get` collides
/// with `Headers.get`, `Map.get`, and `URLSearchParams.get`, none of which have
/// anything to do with environment variables. Requiring the owners to agree
/// kills that whole class of coincidence.
///
/// The two languages spell the same owner differently, so both sides normalize
/// to a common key: case and underscores are dropped, node's module decoration
/// is stripped, and a short table covers the places where the spellings simply
/// differ — BAML's `T[]` against TypeScript's `Array`, BAML's three numeric
/// types against TypeScript's one `Number`.
///
/// This is a heuristic and it is deliberately biased toward precision. It will
/// miss real pairs whose owners do not correspond (`baml.time.Instant.now`
/// against `Date.now`); those are what the model passes are for. A false match
/// is worse, because it arrives already labelled as agreement.
function owner_key(name: string) -> string {
    let normalized = name.to_lower_case().replace_all("node:", "").replace_all("/promises", "").replace_all("_", "");
    match (normalized) {
        "t[]" => "array",
        "readonlyarray" => "array",
        "map<k, v>" => "map",
        // BAML splits the numeric tower TypeScript keeps as one `number`.
        "int" => "number",
        "float" => "number",
        "bigint" => "number",
        "bool" => "boolean",
        _ => normalized,
    }
}

/// A BAML symbol's owner: the type it is declared on, the namespace it lives
/// in, or — for an impl method — the receiver the impl is written for.
function baml_owner_key(symbol: BamlSymbol) -> string {
    let names: string[] = [];
    let base = "";
    for (let step in symbol.symbol) {
        match (step) {
            let plain: string => {
                names.push(plain);
            },
            let node: ImplStep => {
                base = node.base;
            },
        }
    }
    if (base.length() > 0) {
        owner_key(base)
    } else if (names.length() > 1) {
        owner_key(type_display_name(names.slice(0, names.length() - 1).join(".")))
    } else {
        ""
    }
}

/// The owner a TypeScript symbol shares with its BAML counterpart.
///
/// A member's owner is the container that holds it — `Foo` in
/// `[(globals), Foo, indexOf]` — never the module, which groups but does not
/// own. Something reached on the module itself (`os.hostname`, `parseInt`) has
/// the module for an owner, and a container has none at all: it *is* one, and
/// two containers agreeing on a name is a different question.
function ts_owner_key(symbol: TsSymbol) -> string {
    if (symbol.origin == "container") {
        ""
    } else if (symbol.symbol.length() >= 3) {
        owner_key(symbol.symbol.at(1) ?? "")
    } else {
        owner_key(symbol.symbol.at(0) ?? "")
    }
}

/// A BAML symbol's own name. An impl method matches on its method name, not on
/// the interface it arrives through.
function baml_name_of(symbol: BamlSymbol) -> string {
    let name = "";
    for (let step in symbol.symbol) {
        match (step) {
            let plain: string => {
                name = plain;
            },
            _ => {},
        }
    }
    name
}

/// Builds the matrix from both symbol layers using deterministic name matching
/// only.
///
/// A name match is a *proposal*, not a conclusion: it arrives unverified, and
/// the verify pass is expected to look at it like any other. Name agreement
/// after normalization is a good heuristic and nothing more — `baml.env.get`
/// and `Headers.get` agree on their name and on nothing else.
///
/// Symbols this pass says nothing about get no judgement at all. Silence and a
/// judged absence are different states, and conflating them would make the
/// first run indistinguishable from one where a model considered every symbol
/// and found nothing.
function build_symbol_matrix(
    export_doc: PackageExportDoc,
    ts_doc: TsSurfaceDoc,
    baml_surface_sha256: string,
) -> SymbolMatrix {
    let baml_symbols = extract_symbols(export_doc);
    let ts_symbols = extract_ts_symbols(ts_doc);

    let by_key: map<string, BamlSymbol[]> = {};
    for (let symbol in baml_symbols) {
        let key = match_key(baml_name_of(symbol));
        let bucket: BamlSymbol[] = by_key.get(key) ?? [];
        bucket.push(symbol);
        let _ = by_key.set(key, bucket);
    }

    let judgements: Judgement[] = [];
    for (let symbol in ts_symbols) {
        let owner = ts_owner_key(symbol);
        // A container has no owner to agree about, and two containers sharing a
        // name is a coincidence rather than a correspondence.
        if (owner.length() == 0) {
            continue;
        }
        let bucket: BamlSymbol[] = by_key.get(match_key(ts_name_of(symbol))) ?? [];
        let counterparts: string[] = [];
        for (let candidate in bucket) {
            // The name agreeing is not enough; the owners must too.
            if (owner == baml_owner_key(candidate)) {
                counterparts.push(candidate.id);
            }
        }
        if (counterparts.length() > 0) {
            let none: string[] = [];
            judgements
                .push(
                    Judgement {
                        ts: symbol.id,
                        baml: counterparts,
                        verdict: "match",
                        basis: "name",
                        confidence: null,
                        reason: null,
                        divergence: null,
                        rejected: none,
                        verified: false,
                        example: null,
                    },
                );
        }
    }

    let no_failures: PassFailure[] = [];
    let matrix = SymbolMatrix {
        format_version: 2,
        provenance: MatrixProvenance {
            baml_surface_sha256: baml_surface_sha256,
            export_format_version: export_doc.format_version,
            typescript_version: ts_doc.typescript_version,
            types_node_version: ts_doc.types_node_version,
        },
        baml: baml_symbols,
        ts: ts_symbols,
        judgements: judgements,
        failures: no_failures,
        counts: MatrixCounts {
            baml_symbols: 0,
            ts_symbols: 0,
            judgements: 0,
            matched: 0,
            unnecessary: 0,
            unmatched: 0,
            unjudged: 0,
            baml_unclaimed: 0,
        },
    };
    matrix.counts = recount(matrix);
    matrix
}

/// Build the symbol matrix from the generated data files.
function build_matrix_v2(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
    baml_surface_sha256: string = "",
) -> SymbolMatrix {
    if (baml_surface_sha256.length() == 0) {
        throw baml.errors.InvalidArgument {
            message: "baml_surface_sha256 is required — the run wrapper computes it",
        };
    }
    build_symbol_matrix(load_baml_surface(baml_surface_file), load_ts_surface(ts_surface_file), baml_surface_sha256)
}
