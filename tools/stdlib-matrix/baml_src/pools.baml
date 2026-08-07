// Which TypeScript symbols are worth asking about.
//
// The first version of the model pass put the entire unmatched TypeScript
// surface — some 550 symbols — in front of the model on every call, and asked
// it to find the needle. That is the wrong shape of question: recall falls off
// as the menu grows, and a model asked to search a wall of text will oblige by
// inventing a plausible answer. Narrowing the menu is most of the fix.
//
// A pool is built deterministically, from two sources that fail differently:
//
//   - The correspondence table below, which says where in TypeScript a given
//     part of BAML would live. It is precise and incomplete — it cannot know
//     that `baml.string.split` corresponds to something outside `String`.
//   - A lexical shortlist across the whole surface, which is imprecise and
//     broad. It catches what the table misses.
//
// An empty pool is an answer rather than a failure: nothing plausible exists to
// ask about, and that is recorded without spending a call. Nothing is dropped
// silently — a pool that hits the cap says how much it lost, and that number
// travels with the run.
//
// This table is also the honest statement of what this matrix compares. Two
// libraries have no canonical alignment; a claim that `baml.time` corresponds
// to `Date` is a judgement about scope, and it belongs somewhere a reader can
// find and argue with rather than implied by whatever the model happened to
// consider.

/// The largest pool a single call will be given. Past this the menu stops
/// helping for the same reason the whole surface did.
function pool_cap() -> int {
    120
}

/// The shortest name worth matching lexically. `get`, `at`, and `to` appear all
/// over both libraries and agreeing on them means nothing.
function shortlist_min_length() -> int {
    4
}

/// Where in TypeScript a given part of BAML would live, if anywhere.
///
/// Keyed by the owner as BAML writes it — the primitive spelling for companion
/// classes (`string`, not `baml.String`), the dotted path otherwise. Callers go
/// through `pool_containers`, which walks up the namespace, so a namespace entry
/// covers every type inside it and only the exceptions need their own line.
///
/// An empty list is a real answer: `baml.csv`, `baml.toml`, and `baml.llm` have
/// no TypeScript counterpart at all, and saying so is more useful than pointing
/// the model at something adjacent.
function containers_for_owner(owner: string) -> string[] {
    match (owner) {
        "string" => ["String"],
        "int" => ["Number", "Math"],
        "float" => ["Number", "Math"],
        "bigint" => ["BigInt", "Number"],
        "bool" => ["Boolean"],
        "T[]" => ["Array", "ReadonlyArray"],
        "map<K, V>" => ["Map", "Object"],
        "uint8array" => ["Uint8Array", "ArrayBuffer"],
        "baml.TaggedString" => ["String"],
        // Whole namespaces.
        "baml.json" => ["JSON"],
        "baml.time" => ["Date"],
        "baml.fs" => ["node:fs/promises", "node:path"],
        "baml.glob" => ["node:path", "node:fs/promises"],
        "baml.net" => ["node:net"],
        "baml.ws" => ["WebSocket"],
        "baml.env" => ["node:process"],
        "baml.sys" => ["node:process", "node:child_process", "(globals)"],
        "baml.host" => ["node:process", "node:child_process"],
        "baml.random" => ["Math", "Crypto"],
        "baml.id" => ["Crypto"],
        "baml.iter" => ["Iterator", "Array"],
        "baml.errors" => ["Error"],
        "baml.panics" => ["Error"],
        "baml.future" => ["Promise"],
        "baml.spawn" => ["Promise"],
        "baml.stream" => ["Iterator", "(globals)"],
        "baml.events" => ["AbortController", "AbortSignal"],
        "baml.media" => ["Blob", "File"],
        // Types whose counterpart is not their namespace's.
        "baml.http" => ["(globals)", "Request", "Response", "Headers", "URL"],
        "baml.http.Request" => ["Request"],
        "baml.http.Response" => ["Response"],
        "baml.http.Headers" => ["Headers"],
        "baml.http.Url" => ["URL", "URLSearchParams"],
        // Deliberately empty: BAML surface with no TypeScript counterpart to
        // search. Listed rather than omitted so the absence is a decision.
        "baml.csv" => [],
        "baml.toml" => [],
        "baml.yaml" => [],
        "baml.llm" => [],
        "baml.ops" => [],
        "baml.schema" => [],
        _ => {
            let none: string[] = [];
            none
        },
    }
}

/// The containers to search for a symbol owned by `owner`, walking outward
/// until the table has something to say. `baml.time.ZonedDateTime` finds
/// nothing of its own and inherits `baml.time`'s answer.
function pool_containers(owner: string) -> string[] {
    let segments = owner.split(".");
    let depth = segments.length();
    let found = containers_for_owner(owner);
    while (found.length() == 0 && depth > 1) {
        depth -= 1;
        found = containers_for_owner(segments.slice(0, depth).join("."));
    }
    found
}

/// The owner a symbol belongs to, as BAML writes it.
///
/// An impl method belongs to the type the impl is written for, not to a
/// namespace: `(float[] as baml.FloatStats).mean` is an array operation, and
/// the array is what has a TypeScript counterpart.
function baml_owner_display(symbol: BamlSymbol) -> string {
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
        normalize_impl_base(base)
    } else if (names.length() > 1) {
        // The qualified path, not the short name: the table is keyed on where a
        // symbol lives, and `ZonedDateTime` alone says nothing about `baml.time`
        // being the thing with a TypeScript counterpart. Companion classes are
        // the exception, since the type they back is how they are written.
        normalize_ty_display(names.slice(0, names.length() - 1).join("."))
    } else {
        ""
    }
}

/// An impl's receiver reduced to the type that owns it. `float[]` and `T[]` are
/// both the array type; `map<string, V>` and `map<K, V>` both the map.
function normalize_impl_base(base: string) -> string {
    let trimmed = base.trim();
    if (trimmed.ends_with("[]")) {
        "T[]"
    } else if (trimmed.starts_with("map<")) {
        "map<K, V>"
    } else if let open: int = trimmed.index_of("<") {
        normalize_ty_display(trimmed.substring(0, open))
    } else {
        normalize_ty_display(trimmed)
    }
}

/// Whether two member names share enough of a stem to be worth showing to a
/// model that will decide for itself.
///
/// Containment rather than equality, because the languages spell the same
/// operation at different lengths — `parse` against `parseInt`, `to_string`
/// against `toString`. Short names are excluded outright: `get` agrees with
/// `get` across a dozen unrelated containers, which is the collision the
/// deterministic pass already gates on owners to avoid.
function names_share_a_stem(baml_name: string, ts_name: string) -> bool {
    stems_share(match_key(baml_name), match_key(ts_name))
}

/// The same question over keys that are already normalized.
///
/// Building pools compares every candidate against every member, so normalizing
/// inside the comparison redoes the same lowercasing millions of times — enough
/// to take the pass from seconds to minutes. Callers normalize once per symbol
/// and pass the key.
function stems_share(a: string, b: string) -> bool {
    let shorter = if (a.length() < b.length()) {
        a
    } else {
        b
    };
    if (shorter.length() < shortlist_min_length()) {
        false
    } else {
        a.includes(b) || b.includes(a)
    }
}

/// Each symbol's match key, computed once for the whole surface.
function baml_match_keys(symbols: BamlSymbol[]) -> string[] {
    let keys: string[] = [];
    for (let symbol in symbols) {
        keys.push(match_key(baml_name_of(symbol)));
    }
    keys
}

function ts_match_keys(symbols: TsSymbol[]) -> string[] {
    let keys: string[] = [];
    for (let symbol in symbols) {
        keys.push(match_key(ts_name_of(symbol)));
    }
    keys
}

/// The TypeScript symbols one call will be shown, and what it cost to build.
class CandidatePool {
    owner: string,
    /// The containers the table judged comparable, for the record.
    containers: string[],
    ts_ids: string[],
    /// Candidates the cap excluded. Reported rather than swallowed: a truncated
    /// pool that reads as a complete one turns "we did not ask" into "there is
    /// nothing there".
    dropped: int,
}

/// Builds the pool for one owner's worth of symbols.
///
/// Container members come first and are never dropped: they are the strongest
/// signal, and a cap that cut them would defeat the table. The lexical
/// shortlist fills whatever room is left, in surface order so that two runs
/// over the same input build the same pool.
///
/// A TypeScript symbol another BAML symbol already claims stays in the pool.
/// The two libraries are not in bijection — BAML splits `JSON.stringify` into
/// `stringify` and `stringify_pretty`, and `Date.now` answers for more than one
/// of `baml.time` — so excluding claimed symbols hides the right answer from
/// the second caller. It also makes a pool a fact about the two surfaces rather
/// than about how far this run has got, which is what keeps it identical
/// between a cold run and one resuming from a previous revision.
function build_pool(owner: string, members: BamlSymbol[], ts_symbols: TsSymbol[]) -> CandidatePool {
    build_pool_with_keys(owner, members, ts_symbols, ts_match_keys(ts_symbols))
}

/// The same, with the TypeScript side already normalized — the form
/// `build_pools` uses, so 183 owners share one pass over the surface.
function build_pool_with_keys(
    owner: string,
    members: BamlSymbol[],
    ts_symbols: TsSymbol[],
    ts_keys: string[],
) -> CandidatePool {
    let containers = pool_containers(owner);
    let ids: string[] = [];
    let taken: map<string, bool> = {};

    for (let symbol in ts_symbols) {
        if (containers.includes(symbol.symbol.at(0) ?? "")) {
            let _ = taken.set(symbol.id, true);
            ids.push(symbol.id);
        }
    }

    let member_keys = baml_match_keys(members);
    let dropped = 0;
    let index = 0;
    for (let symbol in ts_symbols) {
        let key = ts_keys.at(index) ?? "";
        index += 1;
        if (taken.has(symbol.id)) {
            continue;
        }
        let shares = false;
        for (let member_key in member_keys) {
            if (!shares && stems_share(member_key, key)) {
                shares = true;
            }
        }
        if (!shares) {
            continue;
        }
        if (ids.length() >= pool_cap()) {
            dropped += 1;
        } else {
            let _ = taken.set(symbol.id, true);
            ids.push(symbol.id);
        }
    }

    CandidatePool { owner: owner, containers: containers, ts_ids: ids, dropped: dropped }
}

/// One pool per owner, over the symbols a pass still has to judge.
///
/// Grouping by owner is what makes the passes affordable — one call per owner
/// rather than one per symbol — and an owner's members share enough context
/// that a model can reason about them as a set.
function build_pools(matrix: SymbolMatrix, pending: BamlSymbol[]) -> CandidatePool[] {
    let groups: map<string, BamlSymbol[]> = {};
    let order: string[] = [];
    for (let symbol in pending) {
        let owner = baml_owner_display(symbol);
        if (!groups.has(owner)) {
            let empty: BamlSymbol[] = [];
            let _ = groups.set(owner, empty);
            order.push(owner);
        }
        if let bucket: BamlSymbol[] = groups.get(owner) {
            bucket.push(symbol);
        }
    }

    let ts_keys = ts_match_keys(matrix.ts);
    let pools: CandidatePool[] = [];
    for (let owner in order) {
        let members: BamlSymbol[] = groups.get(owner) ?? [];
        pools.push(build_pool_with_keys(owner, members, matrix.ts, ts_keys));
    }
    pools
}

/// Every BAML symbol without a counterpart: unjudged, or judged to have none.
///
/// The sweep's universe. A symbol judged `none` belongs here — overturning that
/// from the other direction is the whole reason the sweep exists, since the
/// judgement was made against a pool that evidently did not hold the answer.
/// A symbol already paired does not: it has its counterpart.
function unpaired_symbols(matrix: SymbolMatrix) -> BamlSymbol[] {
    let paired: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        if (judgement_pairs(judgement)) {
            let _ = paired.set(judgement.baml, true);
        }
    }
    matrix.baml.filter((symbol) -> {
        !paired.has(symbol.id)
    })
}

/// Every BAML symbol no judgement speaks for yet.
function pending_symbols(matrix: SymbolMatrix) -> BamlSymbol[] {
    let judged: map<string, bool> = {};
    for (let judgement in matrix.judgements) {
        let _ = judged.set(judgement.baml, true);
    }
    matrix.baml.filter((symbol) -> {
        !judged.has(symbol.id)
    })
}
