// What a symbol depends on: the other symbols its declaration names.
//
// This exists so a run can tell what actually changed since a previous
// revision. A symbol whose own text is untouched can still have changed
// meaning — `read(path: string) -> baml.fs.DirEntry[]` means something new the
// moment `DirEntry` gains a field — so freshness has to look one step out. That
// step is what these references record.
//
// Resolution is by name against declarations, the way the compiler does it:
// a dotted name says where it lives; an unqualified one is tried against each
// enclosing scope, innermost first, before being taken as global. Matching bare
// names across the whole surface is what pairs `Item` in `baml.iter` with an
// unrelated `Item` in `baml.toml`, so it is never done.
//
// The report carries the resolved ids, which makes this the one place the
// resolution rule lives — the web view reads the answer rather than
// recomputing it.

/// Names that a reference can land on, and what they address.
///
/// `types` holds declarations under every name they can be written as: a dotted
/// path, and for a companion class the primitive spelling that appears in
/// signatures (`string`, `T[]`, `map<K, V>`, and `map` alone). `all` holds every
/// symbol by its dotted path, for a name that has already said where it lives.
/// An empty id marks a name more than one declaration answers to — no better
/// than an unknown name, since there is no single thing to point at.
class NameIndex {
    types: map<string, string>,
    all: map<string, string>,
}

function index_put(names: map<string, string>, key: string, id: string) -> null {
    if (key.length() == 0) {
        return null;
    }
    match (names.get(key)) {
        null => {
            let _ = names.set(key, id);
        },
        let existing: string => {
            if (existing != id) {
                let _ = names.set(key, "");
            }
        },
    }
    null
}

function index_get(names: map<string, string>, key: string) -> string? {
    let out: string? = null;
    if let found: string = names.get(key) {
        if (found.length() > 0) {
            out = found;
        }
    }
    out
}

/// A character that can appear in a type's name. The dot is included so a
/// dotted path stays one name rather than becoming three.
function is_name_char(c: string) -> bool {
    c.is_ascii_alphanumeric() || c == "_" || c == "$" || c == "."
}

function trim_dots(name: string) -> string {
    let start = 0;
    let end = name.length();
    while (start < end && name.at(start) == ".") {
        start += 1;
    }
    while (end > start && name.at(end - 1) == ".") {
        end -= 1;
    }
    name.slice(start, end)
}

/// Every name a rendered type mentions, in order.
///
/// A rendered type is text — `map<string, baml.json.json>`,
/// `(Self as baml.Comparable).CompareError`, `"r" | "w"` — so the names are
/// lifted out by splitting on everything that cannot be part of one. The
/// contents of a string-literal type survive as names that resolve to nothing,
/// which costs a lookup and never produces a wrong answer.
function type_names(text: string) -> string[] {
    let names: string[] = [];
    let current = "";
    for (let c in text.chars()) {
        if (is_name_char(c)) {
            current = `${current}${c}`;
        } else {
            let name = trim_dots(current);
            if (name.length() > 0) {
                names.push(name);
            }
            current = "";
        }
    }
    let last = trim_dots(current);
    if (last.length() > 0) {
        names.push(last);
    }
    names
}

/// What a name written inside `scope` refers to.
///
/// The scope walk is the compiler's: `Future` inside `baml.future.Future.then`
/// finds `baml.future.Future` without being written out. A name that carries a
/// dot has already said where it lives and may reach any symbol, member
/// included; a bare one may only reach a declared type.
function resolve_reference(index: NameIndex, name: string, scope: string[]) -> string? {
    let depth = scope.length();
    let found: string? = null;
    while (depth > 0 && found == null) {
        found = index_get(index.types, `${scope.slice(0, depth).join(".")}.${name}`);
        depth -= 1;
    }
    if (found == null) {
        found = index_get(index.types, name);
    }
    if (found == null && name.includes(".")) {
        found = index_get(index.all, name);
    }
    found
}

/// Collects the ids named by a set of rendered types, skipping the names the
/// declaration binds itself (its own type parameters, and `Self`) and skipping
/// itself — a symbol is not its own dependency.
function resolve_all(
    index: NameIndex,
    texts: string[],
    scope: string[],
    shadowed: string[],
    self_id: string,
) -> string[] {
    let ids: string[] = [];
    let seen: map<string, bool> = {};
    for (let text in texts) {
        for (let name in type_names(text)) {
            if (name == "Self" || shadowed.includes(name)) {
                continue;
            }
            if let id: string = resolve_reference(index, name, scope) {
                if (id != self_id && !seen.has(id)) {
                    let _ = seen.set(id, true);
                    ids.push(id);
                }
            }
        }
    }
    ids
}

/// The name a declared generic binds, from `T` or `T: baml.Comparable`.
function generic_param_name(declared: string) -> string {
    let out = declared;
    if let cut: int = declared.index_of(":") {
        out = declared.slice(0, cut);
    }
    out.trim()
}

/// The bound half, when a generic declares one — itself a type reference.
function generic_param_bound(declared: string) -> string {
    let out = "";
    if let cut: int = declared.index_of(":") {
        out = declared.slice(cut + 1, declared.length()).trim();
    }
    out
}

// ── the BAML side ───────────────────────────────────────────────────────────

/// The dotted path an id addresses, or the empty string when it addresses
/// something no name can reach. An impl entry is addressed through its block
/// (`M:(baml.Foo as baml.Comparable).compare`), which is a way to *call* the
/// method rather than a path a signature could mention.
function id_path(id: string) -> string {
    let path = strip_kind_prefix(id);
    if (path.includes("[") || path.starts_with("(")) {
        ""
    } else {
        path
    }
}

function baml_name_index(symbols: BamlSymbol[]) -> NameIndex {
    let index = NameIndex { types: {}, all: {} };
    for (let symbol in symbols) {
        let path = id_path(symbol.id);
        let _ = index_put(index.all, path, symbol.id);
        if (symbol.origin != "type") {
            continue;
        }
        let _ = index_put(index.types, path, symbol.id);
        // A companion class is written as the type it backs, and that spelling
        // is the one signatures use: `string`, not `baml.String`.
        if (symbol.display != last_segment(path)) {
            let _ = index_put(index.types, symbol.display, symbol.id);
            if let open: int = symbol.display.index_of("<") {
                let _ = index_put(index.types, symbol.display.slice(0, open), symbol.id);
            }
        }
    }
    index
}

/// Everything a BAML declaration names: its annotation or target, every type in
/// its signature, and its generic bounds.
function baml_reference_texts(symbol: BamlSymbol) -> string[] {
    let texts: string[] = [];
    if let ty: string = symbol.ty {
        texts.push(ty);
    }
    if let resolved: string = symbol.resolved {
        texts.push(resolved);
    }
    if let fallback: string = symbol.default {
        texts.push(fallback);
    }
    for (let declared in symbol.generics) {
        let bound = generic_param_bound(declared);
        if (bound.length() > 0) {
            texts.push(bound);
        }
    }
    if let signature: SymbolSignature = symbol.signature {
        for (let arg in signature.args) {
            texts.push(arg.ty);
        }
        for (let name in signature.kwargs.keys()) {
            texts.push(signature.kwargs.get(name) ?? "");
        }
        texts.push(signature.returns);
        texts.push(signature.errors);
        for (let declared in signature.generic_params) {
            let bound = generic_param_bound(declared);
            if (bound.length() > 0) {
                texts.push(bound);
            }
        }
    }
    texts
}

/// The scope a BAML symbol's signature was written in: everything enclosing it.
/// An impl method's path names its receiver rather than a namespace, so it has
/// no scope to walk and its signature is qualified throughout.
function baml_scope(symbol: BamlSymbol) -> string[] {
    let names: string[] = [];
    for (let step in symbol.symbol) {
        match (step) {
            let plain: string => {
                names.push(plain);
            },
            _ => {},
        }
    }
    if (names.length() > 1) {
        names.slice(0, names.length() - 1)
    } else {
        let empty: string[] = [];
        empty
    }
}

function baml_shadowed(symbol: BamlSymbol) -> string[] {
    let names: string[] = [];
    for (let declared in symbol.generics) {
        names.push(generic_param_name(declared));
    }
    if let signature: SymbolSignature = symbol.signature {
        for (let declared in signature.generic_params) {
            names.push(generic_param_name(declared));
        }
    }
    names
}

/// Fills in each symbol's references, in place. Runs after extraction because
/// resolution needs the whole set — a symbol can name one declared after it.
function attach_baml_references(symbols: BamlSymbol[]) -> null {
    let index = baml_name_index(symbols);
    for (let symbol in symbols) {
        symbol.references
            = resolve_all(
                index,
                baml_reference_texts(symbol),
                baml_scope(symbol),
                baml_shadowed(symbol),
                symbol.id,
            );
    }
    null
}

// ── the TypeScript side ─────────────────────────────────────────────────────

/// Containers are records of their own, so a type name in a signature points at
/// one. Everything is registered under its id; a container additionally claims
/// its bare name, but only when it is global.
///
/// That restriction is TypeScript's own rule rather than a tie-break. `node:url`
/// exports a `URL` class and so does the web platform, but a signature that
/// writes `URL` without importing it means the global one — so the bare name
/// belongs to `(globals)` and `(web)`, and a module's container is reached by
/// its qualified id.
function ts_name_index(symbols: TsSymbol[]) -> NameIndex {
    let index = NameIndex { types: {}, all: {} };
    for (let symbol in symbols) {
        let _ = index_put(index.all, symbol.id, symbol.id);
        if (symbol.origin == "container") {
            let _ = index_put(index.types, symbol.id, symbol.id);
            let module = symbol.symbol.at(0) ?? "";
            if (module == "(globals)" || module == "(web)") {
                let _ = index_put(index.types, symbol.symbol.at(1) ?? "", symbol.id);
            }
        }
    }
    index
}

/// No scope walk on this side: a TypeScript signature is printed text, so its
/// parameter names sit in the same string as its types. Resolving them against
/// the enclosing container would make `message` in `Error(message?: string)`
/// point at the `Error.message` property.
function attach_ts_references(symbols: TsSymbol[]) -> null {
    let index = ts_name_index(symbols);
    let nothing: string[] = [];
    for (let symbol in symbols) {
        let texts: string[] = [symbol.signature];
        // The module is the scope. A node signature writes `Readable`, not
        // `node:stream.Readable`, and only global containers claim a bare name
        // — so without this, none of the 59 node containers was reachable from
        // any signature and every node symbol's `references` came back empty.
        // That is not a cosmetic gap: `is_fresh` reads references to decide
        // whether a judgement may be carried, so a node symbol looked fresh
        // even when a type it names had changed.
        //
        // `resolve_reference` walks the scope by joining with a dot, so this
        // tries `node:stream.Readable` before falling back to the bare name.
        // Cross-module bare matching is not reintroduced: `(globals)` and
        // `(web)` prefixes match nothing, and those containers are found by the
        // bare-name fallback they already own.
        let scope: string[] = [symbol.symbol.at(0) ?? ""];
        symbol.references = resolve_all(index, texts, scope, nothing, symbol.id);
    }
    null
}
