// The matrix scope: which BAML containers are compared against which
// TypeScript containers, and which surfaces on each side are deliberately
// uncompared. This is the file to update as either stdlib evolves —
// mapping_gaps fails the run when a namespace, top-level class, or TS
// container is missing from both the rows and the uncompared lists, and when
// a row or uncompared entry references something that no longer exists.

function pair(
    key: string,
    baml_types: string[],
    baml_namespaces: string[],
    ts_containers: string[],
    note: string?,
) -> ContainerPair {
    ContainerPair {
        key: key,
        baml_types: baml_types,
        baml_namespaces: baml_namespaces,
        ts_containers: ts_containers,
        synonyms: [],
        note: note,
    }
}

function uncompared(name: string, reason: string) -> UncomparedEntry {
    UncomparedEntry { name: name, reason: reason }
}

function container_mapping() -> ContainerPair[] {
    [
        pair("string", ["baml.String"], [], ["String"], null),
        pair("int", ["baml.Int"], [], ["Number", "Math"], "Math statics are compared against int's numeric methods"),
        pair("float", ["baml.Float"], [], ["Number", "Math"], "Math statics are compared against float's numeric methods"),
        pair("bigint", ["baml.Bigint"], [], ["BigInt"], null),
        pair("bool", ["baml.Bool"], [], ["Boolean"], null),
        pair("array", ["baml.Array"], [], ["Array", "ReadonlyArray"], null),
        pair("map", ["baml.Map"], [], ["Map"], null),
        pair("set", [], [], ["Set"], "BAML has no set container"),
        pair("bytes", ["baml.Uint8Array"], [], ["Uint8Array", "ArrayBuffer", "TextEncoder", "TextDecoder"], "String.from_utf8/to_utf8 also cover the encoder/decoder territory"),
        pair("json", [], ["baml.json"], ["JSON"], null),
        pair("time", [], ["baml.time"], ["Date"], "BAML follows the Temporal split (Instant/PlainDate/ZonedDateTime…) rather than a single Date"),
        pair("future", [], ["baml.future"], ["Promise"], "the extractor folds PromiseConstructor statics (all/race/…) into Promise"),
        pair("iterator", [], ["baml.iter"], ["Iterator"], "BAML's adapter classes (Filter, FlatMap, …) pool alongside the Iterator/Iterable interfaces"),
        pair("error", [], ["baml.errors", "baml.panics"], ["Error"], "BAML splits panics from normal errors; TS has one Error hierarchy"),
        pair("fs", [], ["baml.fs", "baml.glob"], ["node:fs/promises"], null),
        pair("path", [], [], ["node:path"], "BAML has no path-manipulation API"),
        pair("system", [], ["baml.sys", "baml.env"], ["node:process", "node:child_process"], "baml.sys covers both process introspection and subprocess spawning"),
        pair("net", [], ["baml.net"], ["node:net"], null),
        pair("http", [], ["baml.http"], ["Request", "Response", "Headers", "(globals)"], "(globals) carries fetch/structuredClone/atob/btoa; BAML additionally ships an HTTP server and SSE"),
        pair("websocket", [], ["baml.ws"], ["WebSocket"], null),
        pair("url", [], [], ["URL", "URLSearchParams"], "BAML has no URL parsing/manipulation API"),
        pair("regex", [], [], ["RegExp"], "BAML has no regex API (String.matches is a substring test)"),
        pair("random", [], ["baml.random"], ["Crypto"], "BAML has seedable RNGs; Crypto's random side is the closest TS surface"),
        pair("crypto", [], [], ["SubtleCrypto"], "BAML has no hashing/signing API"),
        pair("blob", [], [], ["Blob", "File"], "BAML's closest concept, baml.media, is LLM-oriented and listed as uncompared"),
        pair("cancellation", [], ["baml.spawn"], ["AbortController", "AbortSignal"], "CancelToken vs AbortController; TaskGroup structured concurrency is BAML-only"),
    ]
}

function uncompared_baml_surfaces() -> UncomparedEntry[] {
    [
        uncompared("baml", "root-namespace remainder: core language interfaces (Comparable, Sortable, ToJson, …), conversion machinery, and reflection helpers — language substrate, not a stdlib container"),
        uncompared("baml.Null", "companion of the null literal; JS null has no container"),
        uncompared("baml.TaggedString", "template-string machinery"),
        uncompared("baml.TypeValue", "reflection type values"),
        uncompared("baml.llm", "LLM clients and prompting — no TS-stdlib counterpart"),
        uncompared("baml.csv", "codec beyond JSON; the TS stdlib ships JSON only"),
        uncompared("baml.toml", "codec beyond JSON; the TS stdlib ships JSON only"),
        uncompared("baml.yaml", "codec beyond JSON; the TS stdlib ships JSON only"),
        uncompared("baml.media", "LLM media values (images/audio), not a byte-blob API"),
        uncompared("baml.io", "console/stdio; the TS `console` is not yet in the extractor scope — candidate future row"),
        uncompared("baml.ops", "operator-overload interfaces — language machinery"),
        uncompared("baml.stream", "LLM streaming controls"),
        uncompared("baml.id", "runtime invocation-ID context, not an ID-generation API"),
        uncompared("baml.host", "host-embedding escape hatch"),
        uncompared("baml.sap", "schema-aligned parsing internals"),
        uncompared("baml.schema", "reflection schema"),
    ]
}

function uncompared_ts_surfaces() -> UncomparedEntry[] {
    [
        uncompared("Object", "statics operate on JS plain objects; BAML's nominal classes plus the map row cover that territory"),
        uncompared("Symbol", "well-known-symbol protocol machinery; no BAML counterpart concept"),
    ]
}

/// Returns human-readable diagnostics for every hole in the two-way scope:
/// surfaces present in a document but absent from rows + uncompared lists,
/// and rows/uncompared entries referencing surfaces that do not exist.
/// An empty result means the mapping is total.
function mapping_gaps(
    export_doc: PackageExportDoc,
    ts_doc: TsSurfaceDoc,
    rows: ContainerPair[],
    uncompared_baml: UncomparedEntry[],
    uncompared_ts: UncomparedEntry[],
) -> string[] {
    let gaps: string[] = [];

    let covered_baml: map<string, bool> = {};
    let covered_ts: map<string, bool> = {};
    for (let row in rows) {
        for (let qualified in row.baml_types) {
            let _ = covered_baml.set(qualified, true);
        }
        for (let ns in row.baml_namespaces) {
            let _ = covered_baml.set(ns, true);
        }
        for (let cname in row.ts_containers) {
            let _ = covered_ts.set(cname, true);
        }
    }
    for (let entry in uncompared_baml) {
        let _ = covered_baml.set(entry.name, true);
    }
    for (let entry in uncompared_ts) {
        let _ = covered_ts.set(entry.name, true);
    }

    // Every exported surface must be scoped. Namespaces are scoped as a unit;
    // top-level classes individually (a new root class is a major addition and
    // must be placed deliberately); the remaining root items (interfaces, free
    // functions, enums, aliases) fall under the package-name catch-all entry.
    let seen_gap: map<string, bool> = {};
    for (let item in export_doc.items) {
        if (item.name.includes("$")) {
            continue;
        }
        let container = container_of(item.id);
        let scope_name = container;
        if (container == export_doc.package && item.kind == "class") {
            scope_name = `${container}.${item.name}`;
        }
        if (!covered_baml.has(scope_name) && !seen_gap.has(scope_name)) {
            let _ = seen_gap.set(scope_name, true);
            gaps.push(`unscoped BAML surface: ${scope_name} (add it to a mapping row or the uncompared list)`);
        }
    }
    for (let container in ts_doc.containers) {
        if (!covered_ts.has(container.name)) {
            gaps.push(`unscoped TS container: ${container.name} (add it to a mapping row or the uncompared list)`);
        }
    }

    // Every reference must exist, so renames/removals surface here instead of
    // silently emptying a row.
    let observed_baml: map<string, bool> = {};
    for (let item in export_doc.items) {
        if (item.name.includes("$")) {
            continue;
        }
        let container = container_of(item.id);
        let _ = observed_baml.set(container, true);
        if (container == export_doc.package && item.kind == "class") {
            let _ = observed_baml.set(`${container}.${item.name}`, true);
        }
    }
    let observed_ts: map<string, bool> = {};
    for (let container in ts_doc.containers) {
        let _ = observed_ts.set(container.name, true);
    }
    for (let row in rows) {
        for (let qualified in row.baml_types) {
            if (!observed_baml.has(qualified)) {
                gaps.push(`mapping row ${row.key}: BAML type ${qualified} is not in the export`);
            }
        }
        for (let ns in row.baml_namespaces) {
            if (!observed_baml.has(ns)) {
                gaps.push(`mapping row ${row.key}: BAML namespace ${ns} is not in the export`);
            }
        }
        for (let cname in row.ts_containers) {
            if (!observed_ts.has(cname)) {
                gaps.push(`mapping row ${row.key}: TS container ${cname} is not in the surface document`);
            }
        }
    }
    for (let entry in uncompared_baml) {
        if (!observed_baml.has(entry.name)) {
            gaps.push(`stale uncompared BAML entry: ${entry.name}`);
        }
    }
    for (let entry in uncompared_ts) {
        if (!observed_ts.has(entry.name)) {
            gaps.push(`stale uncompared TS entry: ${entry.name}`);
        }
    }
    gaps
}
