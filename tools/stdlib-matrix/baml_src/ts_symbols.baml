// The TypeScript symbol layer — the counterpart to symbols.baml.
//
// Both sides are flattened to symbol lists so the passes can ask one question
// per symbol rather than per container. The two shapes are deliberately
// parallel where the languages agree (path, display, kind, origin, doc) and
// honest where they do not: TypeScript declares no error contract, so there is
// no `errors` field to fill, and its signatures are printed text rather than a
// resolved structure, so `signature` is a string rather than parts.
//
// Containers are symbols here, not merely headings. `Date` is a thing the
// matrix has an opinion about — it corresponds to `baml.time.Instant` — and an
// opinion needs somewhere to live. It is also the unit the alignment passes
// work over: one session per container, answering for the container and every
// member it holds.

/// A TypeScript symbol: a module's container, a module's function, or a
/// container's member.
///
/// `symbol` is `[module, container]`, `[module, function]`, or
/// `[module, container, member]` — three levels, the way a reader reaches it.
class TsSymbol {
    /// This symbol's address, written the way TypeScript addresses it — see
    /// `ts_member_id`.
    id: string,
    symbol: string[],
    display: string,
    /// What it is, as declared: `class`, `interface`, `namespace`, `function`,
    /// `method`, `static method`, `property`.
    kind: string,
    /// How it is reached, and how the view nests it: `container` for a type,
    /// `free` for something reached on the module, and `method` /
    /// `static_method` / `property` for a container's members.
    origin: string,
    signature: string,
    doc: string?,
    /// The lib file that introduced it (`es5`, `es2015`, `dom`, `node`) — the
    /// closest TypeScript has to a stability marker.
    since: string,
    /// The ids of the symbols this signature names — see references.baml.
    references: string[],
}

/// The prefix a module puts on the addresses inside it.
///
/// The globals and the web platform share one namespace — `URL` is written
/// `URL` wherever you are — so they add nothing. Node modules must qualify:
/// `Server` is declared by `http`, `net`, and `tls`, and three symbols sharing
/// one address is three wrong answers.
function ts_module_prefix(module: TsModule) -> string {
    if (module.kind == "node") {
        `${module.name}.`
    } else {
        ""
    }
}

function ts_container_id(module: TsModule, container: TsContainer) -> string {
    `${ts_module_prefix(module)}${container.name}`
}

function ts_function_id(module: TsModule, declared: TsFunction) -> string {
    `${ts_module_prefix(module)}${declared.name}`
}

/// How TypeScript addresses a member, which is also its id.
///
/// The static and instance sides of a container are separate declarations that
/// may share a name — `Uint8Array.BYTES_PER_ELEMENT` is both a constructor
/// property and an instance one — so a bare `Container.member` is not an
/// address. TypeScript's own answer is the one used here: the static side is
/// reached on the constructor, the instance side through `prototype`. A
/// namespace object has no prototype for anything to hang off.
///
/// Keyed on `static`, not on the member's kind: the constructor side holds
/// constants as well as methods, and both are reached on the constructor.
/// Deciding from the kind put a static constant behind `prototype`, which is
/// how the two sides of `Uint8Array.BYTES_PER_ELEMENT` came to share one
/// address.
function ts_member_id(module: TsModule, container: TsContainer, member: TsMember) -> string {
    let owner = ts_container_id(module, container);
    if ((container.singleton ?? false) || member.static) {
        `${owner}.${member.name}`
    } else {
        `${owner}.prototype.${member.name}`
    }
}

/// An id as a reader would write it. Only the module scheme comes off: `node:`
/// is how you import `os`, not how you call `os.hostname`.
function ts_display_of(id: string) -> string {
    if (id.starts_with("node:")) {
        id.substring(5, id.length())
    } else {
        id
    }
}

/// A printed signature that opens a parameter list is callable; a property
/// prints as `readonly x: T`. Overloads all agree on this, so the first is
/// enough.
function ts_is_callable(signatures: string[]) -> bool {
    let out = false;
    if let first: string = signatures.at(0) {
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

/// How a member is reached, and so how it should be compared.
///
/// Callability is asked first, because the constructor side holds constants as
/// well as statics: `Number.MAX_SAFE_INTEGER` and `Array.prototype` come from
/// the `*Constructor` interface and are not methods. Testing `static` first
/// labelled 95 of them `static_method`, which sends a constant to be compared
/// against BAML's methods rather than its fields.
function ts_member_origin(member: TsMember) -> string {
    if (!ts_is_callable(member.signatures)) {
        "property"
    } else if (member.static) {
        "static_method"
    } else {
        "method"
    }
}

function ts_member_kind(origin: string) -> string {
    match (origin) {
        "static_method" => "static method",
        "property" => "property",
        _ => "method",
    }
}

/// Overloads are joined rather than dropped: `Array.from` has three, and which
/// one corresponds to a BAML symbol is exactly the question a pass asks.
function ts_signature_of(signatures: string[]) -> string {
    signatures.join("\n")
}

/// A container's own signature: its declaration head, synthesized. The lib
/// prints no single line for `interface Date { … }` that is worth keeping, and
/// a reader scanning the tree wants to know what kind of thing it is.
///
/// No member count: the container's members nest under it, so the view can say
/// how many without being told.
function ts_container_signature(container: TsContainer) -> string {
    `${container.kind} ${container.name}`
}

function extract_ts_symbols(doc: TsSurfaceDoc) -> TsSymbol[] {
    let symbols: TsSymbol[] = [];
    for (let module in doc.modules) {
        for (let container in module.containers) {
            let container_id = ts_container_id(module, container);
            symbols.push(
                TsSymbol {
                    id: container_id,
                    symbol: [module.name, container.name],
                    display: ts_display_of(container_id),
                    kind: container.kind,
                    origin: "container",
                    signature: ts_container_signature(container),
                    doc: container.doc,
                    since: container.since,
                    references: [],
                },
            );
            for (let member in container.members) {
                let origin = ts_member_origin(member);
                let member_id = ts_member_id(module, container, member);
                symbols.push(
                    TsSymbol {
                        id: member_id,
                        symbol: [module.name, container.name, member.name],
                        display: ts_display_of(member_id),
                        kind: ts_member_kind(origin),
                        origin: origin,
                        signature: ts_signature_of(member.signatures),
                        doc: member.doc,
                        since: member.since,
                        references: [],
                    },
                );
            }
        }
        for (let declared in module.functions) {
            let function_id = ts_function_id(module, declared);
            symbols.push(
                TsSymbol {
                    id: function_id,
                    symbol: [module.name, declared.name],
                    display: ts_display_of(function_id),
                    kind: if (ts_is_callable(declared.signatures)) {
                        "function"
                    } else {
                        "property"
                    },
                    origin: "free",
                    signature: ts_signature_of(declared.signatures),
                    doc: declared.doc,
                    since: declared.since,
                    references: [],
                },
            );
        }
    }
    attach_ts_references(symbols);
    symbols
}

/// Dump the TypeScript symbol layer for inspection.
function dump_ts_symbols(
    ts_surface_file: string = "tools/stdlib-matrix/data/ts_surface.json",
) -> TsSymbol[] {
    extract_ts_symbols(load_ts_surface(ts_surface_file))
}
