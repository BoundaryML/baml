// The matrix: two symbol lists and the correspondences between them.
//
// The two languages are not 1:1, so neither hierarchy can be the document's
// structure without distorting the other. A BAML method reached through an
// interface impl has no TypeScript shape; a TypeScript container holds members
// scattered across several BAML namespaces. Privileging one would force a
// reader of the other to rebuild their own view out of a foreign tree.
//
// So both sides are stored flat, each symbol carrying its own path, with
// correspondences as a separate relation. A BAML-rooted view groups `baml` by
// namespace; a TypeScript-rooted view groups `ts` by container; both read the
// same correspondences. Neither is privileged and nothing is duplicated.
//
// `key` and `status` are gone: identity is the path, and matching state is
// derivable — a symbol with no correspondences is unmatched.

/// What was concluded about one BAML symbol, and on what grounds.
///
/// Recorded rather than derived, because the interesting part is not the
/// pairing but the reasoning: a symbol judged to have no TypeScript counterpart
/// is a finding about the two libraries, and one judged to have a *divergent*
/// counterpart — the same operation reached differently — is the finding this
/// matrix exists for. Both are indistinguishable from silence unless written
/// down.
///
/// Both sides are ids, never array indices. Indices shift whenever the stdlib
/// does, which makes a report undiffable and a previous revision useless as a
/// cache; ids are content-derived and survive.
///
/// `verdict` is what is true:
///   - `match`      — the same operation, reached the same way
///   - `divergent`  — the same operation, reached differently; `divergence` says how
///   - `none`       — nothing on the other side does this symbol's job
///
/// `basis` is why it is believed:
///   - `name`           — deterministic agreement of name and owner
///   - `model`          — a model judged it, and said why
///   - `carried`        — held over from a previous revision, nothing having changed
///   - `no-candidates`  — no plausible counterpart existed to ask about
class Judgement {
    baml: string,
    ts: string?,
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
    /// BAML symbols with a counterpart, exact or divergent.
    matched: int,
    /// BAML symbols judged to have none — considered, and found wanting.
    unmatched: int,
    /// BAML symbols nothing has judged yet.
    unjudged: int,
    /// TypeScript symbols no judgement claims.
    ts_unclaimed: int,
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

/// Recomputes the tallies from the relation rather than by adjusting the old
/// numbers, so a pass that forgets to increment cannot leave the report lying.
function recount(matrix: SymbolMatrix) -> MatrixCounts {
    let paired: map<string, bool> = {};
    let judged: map<string, bool> = {};
    let ts_claimed: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.baml, true);
        if (judgement_pairs(judgement)) {
            let _ = paired.set(judgement.baml, true);
            if let ts: string = judgement.ts {
                let _ = ts_claimed.set(ts, true);
            }
        }
    }
    let matched = 0;
    let unmatched = 0;
    let unjudged = 0;
    for (let symbol in matrix.baml) {
        if (paired.has(symbol.id)) {
            matched += 1;
        } else if (judged.has(symbol.id)) {
            unmatched += 1;
        } else {
            unjudged += 1;
        }
    }
    let ts_unclaimed = 0;
    for (let symbol in matrix.ts) {
        if (!ts_claimed.has(symbol.id)) {
            ts_unclaimed += 1;
        }
    }
    MatrixCounts {
        baml_symbols: matrix.baml.length(),
        ts_symbols: matrix.ts.length(),
        judgements: matrix.judgements.length(),
        matched: matched,
        unmatched: unmatched,
        unjudged: unjudged,
        ts_unclaimed: ts_unclaimed,
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

function ts_owner_key(symbol: TsSymbol) -> string {
    owner_key(symbol.symbol.at(0) ?? "")
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

    let by_key: map<string, TsSymbol[]> = {};
    for (let symbol in ts_symbols) {
        let key = match_key(ts_name_of(symbol));
        let bucket: TsSymbol[] = by_key.get(key) ?? [];
        bucket.push(symbol);
        let _ = by_key.set(key, bucket);
    }

    let judgements: Judgement[] = [];
    for (let symbol in baml_symbols) {
        let bucket: TsSymbol[] = by_key.get(match_key(baml_name_of(symbol))) ?? [];
        let owner = baml_owner_key(symbol);
        for (let candidate in bucket) {
            // The name agreeing is not enough; the owners must too.
            if (owner.length() > 0 && owner == ts_owner_key(candidate)) {
                let none: string[] = [];
                judgements
                    .push(
                        Judgement {
                            baml: symbol.id,
                            ts: candidate.id,
                            verdict: "match",
                            basis: "name",
                            confidence: null,
                            reason: null,
                            divergence: null,
                            rejected: none,
                            verified: false,
                        },
                    );
            }
        }
    }

    let no_failures: PassFailure[] = [];
    let matrix = SymbolMatrix {
        format_version: 1,
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
            unmatched: 0,
            unjudged: 0,
            ts_unclaimed: 0,
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
