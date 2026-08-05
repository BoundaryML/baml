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

/// A claimed equivalence between one BAML symbol and one TypeScript symbol.
///
/// The two sides are array indices, not names: 15 BAML displays collide (the
/// export disambiguates same-headed impl blocks with a `#n` suffix the display
/// does not carry) and one TypeScript display does. Indices are unambiguous,
/// and both arrays are deterministically ordered.
///
/// `basis` is `name` for deterministic agreement after case and underscore
/// normalization, or `model` for a judgment a later pass made, which also
/// fills in confidence and reasoning.
class Correspondence {
    baml: int,
    ts: int,
    basis: string,
    confidence: string?,
    reason: string?,
}

class MatrixCounts {
    baml_symbols: int,
    ts_symbols: int,
    correspondences: int,
    /// BAML symbols with at least one correspondence.
    baml_matched: int,
    /// BAML symbols with none.
    baml_unmatched: int,
    /// TypeScript symbols no BAML symbol corresponds to.
    ts_unmatched: int,
}

class SymbolMatrix {
    format_version: int,
    provenance: MatrixProvenance,
    baml: BamlSymbol[],
    ts: TsSymbol[],
    correspondences: Correspondence[],
    counts: MatrixCounts,
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
/// only. The model pass enriches this by adding `model`-basis correspondences;
/// it never removes a name match.
function build_symbol_matrix(
    export_doc: PackageExportDoc,
    ts_doc: TsSurfaceDoc,
    baml_surface_sha256: string,
) -> SymbolMatrix {
    let baml_symbols = extract_symbols(export_doc);
    let ts_symbols = extract_ts_symbols(ts_doc);

    // TS symbols bucketed by match key, holding indices rather than values so
    // correspondences can name them.
    let by_key: map<string, int[]> = {};
    let ts_index = 0;
    for (let symbol in ts_symbols) {
        let key = match_key(ts_name_of(symbol));
        let bucket: int[] = by_key.get(key) ?? [];
        bucket.push(ts_index);
        let _ = by_key.set(key, bucket);
        ts_index += 1;
    }

    let correspondences: Correspondence[] = [];
    let ts_claimed: map<string, bool> = {};
    let baml_matched = 0;
    let baml_unmatched = 0;

    let baml_index = 0;
    for (let symbol in baml_symbols) {
        let bucket: int[] = by_key.get(match_key(baml_name_of(symbol))) ?? [];
        let owner = baml_owner_key(symbol);
        let kept = 0;
        for (let candidate in bucket) {
            // The name agreeing is not enough; the owners must too.
            if let ts: TsSymbol = ts_symbols.at(candidate) {
                if (owner.length() > 0 && owner == ts_owner_key(ts)) {
                    correspondences
                        .push(
                            Correspondence {
                                baml: baml_index,
                                ts: candidate,
                                basis: "name",
                                confidence: null,
                                reason: null,
                            },
                        );
                    let _ = ts_claimed.set(candidate.to_string(), true);
                    kept += 1;
                }
            }
        }
        if (kept > 0) {
            baml_matched += 1;
        } else {
            baml_unmatched += 1;
        }
        baml_index += 1;
    }

    let ts_unmatched = 0;
    let scan = 0;
    while (scan < ts_symbols.length()) {
        if (!ts_claimed.has(scan.to_string())) {
            ts_unmatched += 1;
        }
        scan += 1;
    }

    SymbolMatrix {
        format_version: 1,
        provenance: MatrixProvenance {
            baml_surface_sha256: baml_surface_sha256,
            export_format_version: export_doc.format_version,
            typescript_version: ts_doc.typescript_version,
            types_node_version: ts_doc.types_node_version,
        },
        baml: baml_symbols,
        ts: ts_symbols,
        correspondences: correspondences,
        counts: MatrixCounts {
            baml_symbols: baml_symbols.length(),
            ts_symbols: ts_symbols.length(),
            correspondences: correspondences.length(),
            baml_matched: baml_matched,
            baml_unmatched: baml_unmatched,
            ts_unmatched: ts_unmatched,
        },
    }
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
