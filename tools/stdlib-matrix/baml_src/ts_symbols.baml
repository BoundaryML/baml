// The TypeScript symbol layer — the counterpart to symbols.baml.
//
// Both sides are flattened to symbol lists so the passes can ask one question
// per symbol rather than per container. The two shapes are deliberately
// parallel where the languages agree (path, display, origin, doc) and honest
// where they do not: TypeScript declares no error contract, so there is no
// `errors` field to fill, and its signatures are printed text rather than a
// resolved structure, so `signature` is a string rather than parts.

/// A TypeScript symbol: a member of one of the compared containers.
///
/// `path` is `[container, member]` — TypeScript's lib types are flat, with no
/// namespace above the container. `origin` mirrors the BAML side's vocabulary
/// so the two can be compared without translating: `static_method` for the
/// constructor-interface side (`String.fromCodePoint`), `method` for the
/// instance side, `property` for non-callable members, and `free` for the
/// globals and module functions that belong to no container.
class TsSymbol {
    symbol: string[],
    display: string,
    origin: string,
    signature: string,
    doc: string?,
    /// The lib file that introduced it (`es5`, `es2015`, `dom`, `node`) — the
    /// closest TypeScript has to a stability marker.
    since: string,
}

/// `(globals)` and `node:*` hold free functions; everything else is a type
/// whose members are reached on a value.
function ts_origin_of(container: TsContainer, member: TsMember) -> string {
    if (container.kind == "functions" || container.kind == "module") {
        "free"
    } else if (member.static) {
        "static_method"
    } else if (ts_is_callable(member)) {
        "method"
    } else {
        "property"
    }
}

/// A printed signature that opens a parameter list is callable; a property
/// prints as `readonly x: T`. Overloads all agree on this, so the first is
/// enough.
function ts_is_callable(member: TsMember) -> bool {
    let out = false;
    if let first: string = member.signatures.at(0) {
        // `charAt(pos: number): string` vs `readonly length: number`.
        // A computed name (`[Symbol.iterator]()`) still shows the paren.
        if let paren: int = first.index_of("(") {
            if let colon: int = first.index_of(":") {
                out = paren < colon;
            } else {
                out = true;
            }
        }
    }
    out
}

/// Overloads are joined rather than dropped: `Array.from` has three, and which
/// one corresponds to a BAML symbol is exactly the question a later pass asks.
function ts_signature_of(member: TsMember) -> string {
    member.signatures.join("\n")
}

function extract_ts_symbols(doc: TsSurfaceDoc) -> TsSymbol[] {
    let symbols: TsSymbol[] = [];
    for (let container in doc.containers) {
        for (let member in container.members) {
            symbols
                .push(
                    TsSymbol {
                        symbol: [container.name, member.name],
                        // Container-qualified throughout, including the `(globals)` and
                        // `node:*` pseudo-containers, so a display always says where a
                        // symbol came from.
                        display: `${container.name}.${member.name}`,
                        origin: ts_origin_of(container, member),
                        signature: ts_signature_of(member),
                        doc: member.doc,
                        since: member.since,
                    },
                );
        }
    }
    symbols
}

/// Dump the TypeScript symbol layer for inspection.
function dump_ts_symbols(
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
) -> TsSymbol[] {
    extract_ts_symbols(load_ts_surface(ts_surface_file))
}
