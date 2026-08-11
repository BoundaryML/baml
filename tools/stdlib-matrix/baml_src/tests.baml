// Unit tests over synthetic fixtures. Export-side fixtures are JSON strings
// (not struct literals) for two reasons: they must go through the real decode
// path to exercise the `baml.FromJson` override on ExportSignature, and
// ExportImpl cannot be built as a literal (its `interface` field name is
// rejected by the parser in literal position).

function fixture_export_text() -> string {
    #"
        {
          "format_version": 1,
          "package": "baml",
          "items": [
            {
              "id": "T:baml.Foo",
              "kind": "class",
              "name": "Foo",
              "docstring": "see \"throws\": here",
              "fields": [{"id": "F:baml.Foo._size", "name": "_size", "ty": {"display": "int"}}],
              "methods": [
                {
                  "id": "M:baml.Foo.index_of",
                  "name": "index_of",
                  "signature": {
                    "params": [{"name": "self", "ty": {"display": "baml.Foo"}}],
                    "returns": {"display": "int"},
                    "throws": {"display": "baml.panics.IndexOutOfBounds"}
                  }
                },
                {
                  "id": "M:baml.Foo.from_parts",
                  "name": "from_parts",
                  "signature": {"params": [{"name": "parts", "ty": {"display": "int"}}], "returns": {"display": "baml.Foo"}}
                },
                {
                  "id": "M:baml.Foo.clone",
                  "name": "clone",
                  "signature": {"params": [{"name": "self", "ty": {"display": "baml.Foo"}}], "returns": {"display": "baml.Foo"}}
                },
                {
                  "id": "M:baml.Foo.sort_helper",
                  "name": "sort_helper",
                  "signature": {"params": [{"name": "self", "ty": {"display": "baml.Foo"}}]}
                }
              ],
              "impls": ["X:baml.impl[baml.Comparable for baml.Foo]"]
            },
            {"id": "T:baml.Foo$stream", "kind": "class", "name": "Foo$stream", "methods": []},
            {
              "id": "V:baml.helper_fn",
              "kind": "function",
              "name": "helper_fn",
              "signature": {"params": [], "returns": {"display": "int"}}
            }
          ],
          "impls": [
            {
              "id": "X:baml.impl[baml.Comparable for baml.Foo]",
              "interface": "baml.Comparable",
              "for_ty": {"display": "baml.Foo"},
              "methods": [
                {
                  "id": "X:baml.impl[baml.Comparable for baml.Foo]::compare",
                  "name": "compare",
                  "signature": {
                    "params": [{"name": "self", "ty": {"display": "baml.Foo"}}],
                    "returns": {"display": "int"},
                    "throws": {"display": "CompareError"}
                  }
                },
                {"id": "X:baml.impl[baml.Comparable for baml.Foo]::index_of", "declared_by": "M:baml.Foo.index_of", "name": "index_of", "signature": {"params": [{"name": "self", "ty": {"display": "baml.Foo"}}]}}
              ]
            }
          ]
        }
    "#
}

function fixture_ts_text() -> string {
    #"
        {
          "format_version": 2,
          "typescript_version": "5.8.3-test",
          "types_node_version": null,
          "modules": [
            {
              "name": "(globals)",
              "kind": "globals",
              "containers": [
                {
                  "name": "Foo",
                  "kind": "class",
                  "singleton": false,
                  "doc": "A fixture container.",
                  "since": "es5",
                  "members": [
                    {"name": "indexOf", "static": false, "signatures": ["indexOf(x: number): number"], "doc": "Finds x.", "since": "es5"},
                    {"name": "only", "static": false, "signatures": ["only(): void"], "doc": null, "since": "es5"},
                    {"name": "sortStuff", "static": false, "signatures": ["sortStuff(): void"], "doc": null, "since": "es2015"},
                    {"name": "clone", "static": false, "signatures": ["clone(): Foo"], "doc": null, "since": "es5"}
                  ]
                }
              ],
              "functions": [
                {"name": "parseFixture", "signatures": ["parseFixture(x: string): number"], "doc": null, "since": "es5"}
              ]
            }
          ]
        }
    "#
}

test "id helpers split kind prefixes and path segments" {
    assert.equal(
        strip_kind_prefix("X:baml.impl[baml.Comparable for baml.Foo]"),
        "baml.impl[baml.Comparable for baml.Foo]",
    );
    assert.equal(last_segment("baml.json"), "json");
}

test "the FromJson override decodes throws and leaves string values intact" {
    let doc = parse_export_text(fixture_export_text());
    assert.equal(doc.items.length(), 3);
    if let foo: ExportItem = doc.items.at(0) {
        assert.equal(foo.docstring ?? "", "see \"throws\": here");
        let methods: ExportMethod[] = foo.methods ?? [];
        if let method: ExportMethod = methods.at(0) {
            if let sig: ExportSignature = method.signature {
                if let spec: TyRef = sig.throws_spec {
                    assert.equal(spec.display, "baml.panics.IndexOutOfBounds");
                } else {
                    baml.sys.panic("throws_spec missing — the FromJson override did not run");
                }
            } else {
                baml.sys.panic("index_of signature missing");
            }
        }
    } else {
        baml.sys.panic("fixture items empty");
    }
}

test "BAML symbols carry paths, origins, and split args from kwargs" {
    let symbols = extract_symbols(parse_export_text(fixture_export_text()));
    let by_display: map<string, BamlSymbol> = {};
    for (let symbol in symbols) {
        let _ = by_display.set(symbol.display, symbol);
    }

    // A `self` receiver makes it an instance method; the owning type is the
    // path's second step.
    if let index_of: BamlSymbol = by_display.get("Foo.index_of") {
        assert.equal(index_of.origin, "method");
        assert.equal(index_of.kind, "function");
        assert.equal(index_of.signature?.errors ?? "", "baml.panics.IndexOutOfBounds");
    } else {
        baml.sys.panic("Foo.index_of missing");
    }

    // No `self` makes it static.
    if let from_parts: BamlSymbol = by_display.get("Foo.from_parts") {
        assert.equal(from_parts.origin, "static_method");
    } else {
        baml.sys.panic("Foo.from_parts missing");
    }

    // Underscore-prefixed members are ordinary surface: BAML has no visibility
    // modifiers, so nothing is filtered on name.
    if let size: BamlSymbol = by_display.get("Foo._size") {
        assert.equal(size.origin, "field");
        assert.equal(size.kind, "field");
    } else {
        baml.sys.panic("Foo._size missing");
    }

    // An impl method's path is [ImplStep, member], never a dotted name.
    let impl_methods = symbols.filter((symbol) -> {
        symbol.origin == "impl_method"
    });
    assert.equal(impl_methods.length(), 2);
}

test "a static that is not callable is a property, not a static method" {
    let constant = TsMember {
        name: "MAX_SAFE_INTEGER",
        static: true,
        signatures: ["readonly MAX_SAFE_INTEGER: number"],
        doc: null,
        since: "es2015",
    };
    // On the constructor side, but a constant. Comparing it against BAML's
    // methods rather than its fields is the wrong question to ask.
    assert.equal(ts_member_origin(constant), "property");
    let factory = TsMember {
        name: "isInteger",
        static: true,
        signatures: ["isInteger(number: unknown): boolean"],
        doc: null,
        since: "es2015",
    };
    assert.equal(ts_member_origin(factory), "static_method");
}

test "TypeScript symbols separate callables from properties" {
    let symbols = extract_ts_symbols(baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()));
    // The container, its four members, and the module's free function. A
    // container is a symbol in its own right: it is what a session is opened
    // over, and what a judgement about `Date` is recorded against.
    assert.equal(symbols.length(), 6);
    if let container: TsSymbol = symbols.at(0) {
        assert.equal(container.display, "Foo");
        assert.equal(container.origin, "container");
        assert.equal(container.kind, "class");
        assert.equal(container.symbol, ["(globals)", "Foo"]);
    } else {
        baml.sys.panic("no TS symbols");
    }
    if let index_of: TsSymbol = symbols.at(1) {
        assert.equal(index_of.display, "Foo.prototype.indexOf");
        assert.equal(index_of.origin, "method");
        assert.equal(index_of.symbol, ["(globals)", "Foo", "indexOf"]);
    } else {
        baml.sys.panic("no member symbol");
    }
    if let free: TsSymbol = symbols.at(5) {
        assert.equal(free.display, "parseFixture");
        assert.equal(free.origin, "free");
        assert.equal(free.kind, "function");
    } else {
        baml.sys.panic("no free function symbol");
    }
}

test "a fresh matrix relates both sides and concludes nothing" {
    let matrix = build_symbol_matrix(
        parse_export_text(fixture_export_text()),
        baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()),
        "sha-fixture",
    );
    assert.equal(matrix.counts.baml_symbols, 9);
    // `Foo`, its four members, and `parseFixture`.
    assert.equal(matrix.counts.ts_symbols, 6);

    // Nothing pairs the two sides deterministically. `Foo.prototype.indexOf`
    // and `baml.Foo.index_of` agree on their name and their owner, and that
    // agreement is no longer taken as evidence of anything — it is the model
    // passes that decide, with reasons attached.
    assert.equal(matrix.judgements.length(), 0);

    // Every symbol is unjudged, which is a different state from judged to have
    // no counterpart: nobody has looked yet.
    assert.equal(matrix.counts.unjudged, matrix.counts.ts_symbols);
    assert.equal(matrix.counts.unmatched, 0);
    assert.equal(matrix.counts.matched, 0);
    assert.equal(matrix.counts.baml_unclaimed, matrix.counts.baml_symbols);
}

// ── ratchet ─────────────────────────────────────────────────────────────────

/// The baseline side of a comparison: what `load_report` would read back.
function fixture_header(sha: string, ts_version: string) -> ReportHeader {
    let matrix = fixture_matrix(sha, ts_version);
    ReportHeader {
        format_version: matrix.format_version,
        provenance: matrix.provenance,
        counts: matrix.counts,
    }
}

/// A fixture that has been judged, standing in for what a model pass produces.
///
/// The correspondences are stated here rather than derived, because nothing
/// derives them any more: `build_symbol_matrix` concludes nothing. That is also
/// the more honest fixture for the revision machinery, whose whole job is
/// carrying model judgements — a heuristic's output was never what it had to
/// preserve.
///
/// `clone` is the one that names a type: its signature returns `Foo`, so it is
/// what proves a judgement goes stale when something it *refers to* moves.
function fixture_matrix(sha: string, ts_version: string) -> SymbolMatrix {
    let matrix = build_symbol_matrix(
        parse_export_text(fixture_export_text()),
        baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()),
        sha,
    );
    matrix.provenance.typescript_version = ts_version;
    matrix.judgements.push(fixture_judgement("Foo.prototype.indexOf", "M:baml.Foo.index_of"));
    matrix.judgements.push(fixture_judgement("Foo.prototype.clone", "M:baml.Foo.clone"));
    matrix.counts = recount(matrix);
    matrix
}

function fixture_judgement(ts: string, baml: string) -> Judgement {
    let none: string[] = [];
    Judgement {
        ts: ts,
        baml: [baml],
        verdict: "match",
        basis: "model",
        confidence: "high",
        reason: "fixture",
        divergence: null,
        rejected: none,
        verified: true,
        example: null,
    }
}

test "an unchanged run matches its baseline" {
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let comparison = compare_to_baseline(fixture_header("sha-a", "5.8.3-test"), matrix);
    assert.is_true(comparison.up_to_date);
    assert.is_true(comparison.baseline_present);
    assert.is_true(!comparison.counts_differ);
    assert.equal(comparison.regressions.length(), 0);
}

test "a changed BAML surface makes the report stale and names the input" {
    let baseline = fixture_header("sha-old", "5.8.3-test");
    let now = fixture_matrix("sha-new", "5.8.3-test");
    let comparison = compare_to_baseline(baseline, now);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.input_changes.length(), 1);
    assert.is_true((comparison.input_changes.at(0) ?? "").includes("BAML stdlib surface changed"));
}

test "a TypeScript version bump is an input change on its own" {
    let baseline = fixture_header("sha-a", "5.8.3-test");
    let now = fixture_matrix("sha-a", "9.9.9-next");
    let comparison = compare_to_baseline(baseline, now);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.input_changes.length(), 1);
    assert.is_true((comparison.input_changes.at(0) ?? "").includes("TypeScript"));
}

test "a missing baseline is reported, not treated as a regression" {
    let comparison = compare_to_baseline(null, fixture_matrix("sha-a", "5.8.3-test"));
    assert.is_true(!comparison.baseline_present);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.regressions.length(), 0);
}

test "the report's header round-trips through JSON" {
    let matrix = fixture_matrix("abc123def456789", "5.8.3-test");
    let decoded = baml.json.to<ReportHeader>(baml.json.parse(baml.json.serialize(matrix)));
    assert.equal(decoded.provenance.baml_surface_sha256, "abc123def456789");
    assert.equal(decoded.counts.baml_symbols, matrix.counts.baml_symbols);
    assert.equal(decoded.counts.judgements, matrix.counts.judgements);
    // A round-tripped header must compare equal to its source, or the ratchet
    // would report drift on every run.
    assert.is_true(compare_to_baseline(decoded, matrix).up_to_date);
    assert.is_true(!compare_to_baseline(decoded, matrix).counts_differ);
}

test "the markdown view renders both sides" {
    let markdown = render_matrix_markdown(fixture_matrix("sha-a", "5.8.3-test"));
    assert.is_true(markdown.includes("# BAML ↔ TypeScript stdlib matrix"));
    // The TypeScript-rooted view, grouped by module, with the BAML side listed
    // after it as what no judgement named.
    assert.is_true(markdown.includes("## TypeScript surface"));
    assert.is_true(markdown.includes("### (globals)"));
    assert.is_true(markdown.includes("BAML symbols no judgement names"));
    assert.is_true(markdown.includes("Foo.prototype.indexOf"));
}

test "a rendered type is split into the names it mentions" {
    assert.equal(type_names("baml.fs.DirEntry[]"), ["baml.fs.DirEntry"]);
    assert.equal(type_names("map<string, unknown>"), ["map", "string", "unknown"]);
    // A dotted path is one name, and the punctuation around a projection does
    // not glue onto either side of it.
    assert.equal(
        type_names("(Self as baml.Comparable).CompareError"),
        ["Self", "as", "baml.Comparable", "CompareError"],
    );
    // String-literal contents survive as names that resolve to nothing.
    assert.equal(type_names("\"r\" | \"w+\""), ["r", "w"]);
}

test "a generic parameter splits from its bound" {
    assert.equal(generic_param_name("T"), "T");
    assert.equal(generic_param_bound("T"), "");
    assert.equal(generic_param_name("I: baml.iter.Iterable"), "I");
    assert.equal(generic_param_bound("I: baml.iter.Iterable"), "baml.iter.Iterable");
}

test "references resolve to declarations, never to members or to nothing" {
    let symbols = extract_symbols(parse_export_text(fixture_export_text()));
    let by_display: map<string, BamlSymbol> = {};
    for (let symbol in symbols) {
        let _ = by_display.set(symbol.display, symbol);
    }

    // `index_of(self: baml.Foo) -> int` names its own receiver and the int
    // companion; `int` is not in this fixture, so only Foo resolves. The symbol
    // is not its own reference.
    if let index_of: BamlSymbol = by_display.get("Foo.index_of") {
        assert.equal(index_of.references, ["T:baml.Foo"]);
    } else {
        baml.sys.panic("Foo.index_of missing");
    }

    // A type declaration names nothing by existing.
    if let foo: BamlSymbol = by_display.get("Foo") {
        let none: string[] = [];
        assert.equal(foo.references, none);
    } else {
        baml.sys.panic("Foo missing");
    }

    // `compare` throws `CompareError`, which nothing in this package declares:
    // an unresolvable name is dropped rather than guessed at.
    let compares = symbols.filter((symbol) -> {
        symbol.origin == "impl_method" && symbol.display.includes("compare")
    });
    if let compare: BamlSymbol = compares.at(0) {
        assert.equal(compare.references, ["T:baml.Foo"]);
    } else {
        baml.sys.panic("the impl's compare is missing");
    }
}

test "a bare name resolves through its own scope, never across the surface" {
    let index = NameIndex { types: {}, all: {} };
    let _ = index_put(index.types, "baml.iter.Chain", "T:baml.iter.Chain");
    let _ = index_put(index.types, "baml.toml.Item", "T:baml.toml.Item");

    // Written inside `baml.iter`, `Chain` finds its neighbour.
    assert.equal(
        resolve_reference(index, "Chain", ["baml", "iter", "ArrayIterator"]) ?? "",
        "T:baml.iter.Chain",
    );
    // `Item` inside `baml.iter` is an associated type, not `baml.toml.Item`.
    // Matching bare names across the whole surface is what would pair them.
    assert.is_true(resolve_reference(index, "Item", ["baml", "iter"]) == null);
    // Qualified, it resolves from anywhere.
    assert.equal(
        resolve_reference(index, "baml.toml.Item", ["baml", "iter"]) ?? "",
        "T:baml.toml.Item",
    );
}

test "a name two declarations answer to points at neither" {
    let index = NameIndex { types: {}, all: {} };
    let _ = index_put(index.types, "Ambiguous", "T:a.Ambiguous");
    let _ = index_put(index.types, "Ambiguous", "T:b.Ambiguous");
    let no_scope: string[] = [];
    assert.is_true(resolve_reference(index, "Ambiguous", no_scope) == null);
}

test "impl entries address themselves and name what they re-list" {
    let symbols = extract_symbols(parse_export_text(fixture_export_text()));
    let impl_methods = symbols.filter((symbol) -> {
        symbol.origin == "impl_method"
    });
    let ids: string[] = [];
    for (let method in impl_methods) {
        ids.push(method.id);
    }
    assert.equal(
        ids,
        [
            "X:baml.impl[baml.Comparable for baml.Foo]::compare",
            "X:baml.impl[baml.Comparable for baml.Foo]::index_of",
        ],
    );
    // The re-listed one carries the declaration it came from; the override
    // declared only in the block has no other address.
    if let relisted: BamlSymbol = impl_methods.at(1) {
        assert.equal(relisted.declared_by ?? "", "M:baml.Foo.index_of");
    } else {
        baml.sys.panic("the impl's index_of is missing");
    }
    if let own: BamlSymbol = impl_methods.at(0) {
        assert.is_true(own.declared_by == null);
    } else {
        baml.sys.panic("the impl's compare is missing");
    }
}

test "every symbol in a matrix is addressed by exactly one id" {
    let matrix = fixture_matrix("sha-ids", "5.8.3-test");
    let seen: map<string, bool> = {};
    let collisions: string[] = [];
    for (let symbol in matrix.baml) {
        if (seen.has(symbol.id)) {
            collisions.push(symbol.id);
        }
        let _ = seen.set(symbol.id, true);
    }
    for (let symbol in matrix.ts) {
        if (seen.has(symbol.id)) {
            collisions.push(symbol.id);
        }
        let _ = seen.set(symbol.id, true);
    }
    // A collision is not a cosmetic flaw: a judgement keyed on a shared id is a
    // statement about whichever symbol the reader happens to find first.
    assert.equal(collisions.join(", "), "");
}

test "the static and instance sides of a container are addressed apart" {
    // `Uint8Array.BYTES_PER_ELEMENT` is declared on both the constructor and
    // the instance. They are different members, and TypeScript reaches them
    // differently.
    let globals = TsModule { name: "(globals)", kind: "globals", containers: [], functions: [] };
    let instance = TsContainer {
        name: "Uint8Array",
        kind: "class",
        singleton: false,
        doc: null,
        since: "es5",
        members: [],
    };
    let member = TsMember {
        name: "BYTES_PER_ELEMENT",
        static: false,
        signatures: ["readonly BYTES_PER_ELEMENT: number"],
        doc: null,
        since: "es5",
    };
    assert.equal(ts_member_id(globals, instance, member), "Uint8Array.prototype.BYTES_PER_ELEMENT");
    // The same name on the constructor side. `static` decides, not the member's
    // kind: the constructor holds constants as well as methods and both are
    // reached on it, so deciding from the kind put a constant behind
    // `prototype` and gave the two sides one address.
    let constructor_side = TsMember {
        name: "BYTES_PER_ELEMENT",
        static: true,
        signatures: ["readonly BYTES_PER_ELEMENT: number"],
        doc: null,
        since: "es5",
    };
    assert.equal(ts_member_id(globals, instance, constructor_side), "Uint8Array.BYTES_PER_ELEMENT");
}

test "node modules qualify their addresses and the globals do not" {
    // `Server` is declared by `http`, `net`, and `tls`. Three symbols sharing
    // one address is three wrong answers, so the module qualifies them; the
    // globals and the web platform share one namespace and add nothing.
    let net = TsModule { name: "node:net", kind: "node", containers: [], functions: [] };
    let globals = TsModule { name: "(globals)", kind: "globals", containers: [], functions: [] };
    let server = TsContainer {
        name: "Server",
        kind: "class",
        singleton: false,
        doc: null,
        since: "node",
        members: [],
    };
    let listen = TsMember {
        name: "listen",
        static: false,
        signatures: ["listen(port: number): this"],
        doc: null,
        since: "node",
    };
    assert.equal(ts_container_id(net, server), "node:net.Server");
    assert.equal(ts_member_id(net, server, listen), "node:net.Server.prototype.listen");
    // The scheme is how you import the module, not how you call into it.
    assert.equal(ts_display_of(ts_member_id(net, server, listen)), "net.Server.prototype.listen");

    let date = TsContainer {
        name: "Date",
        kind: "class",
        singleton: false,
        doc: null,
        since: "es5",
        members: [],
    };
    assert.equal(ts_container_id(globals, date), "Date");
    assert.equal(ts_display_of(ts_container_id(globals, date)), "Date");
}

test "a namespace object's members hang off the value, not a prototype" {
    // `Math.floor`, not `Math.prototype.floor` — there is no prototype for
    // anything to hang off, and an id that says otherwise is a wrong address.
    let globals = TsModule { name: "(globals)", kind: "globals", containers: [], functions: [] };
    let math = TsContainer {
        name: "Math",
        kind: "namespace",
        singleton: true,
        doc: null,
        since: "es5",
        members: [],
    };
    let floor = TsMember {
        name: "floor",
        static: false,
        signatures: ["floor(x: number): number"],
        doc: null,
        since: "es5",
    };
    assert.equal(ts_member_id(globals, math, floor), "Math.floor");
}

// ── carrying a revision forward ─────────────────────────────────────────────

/// A previous revision of the fixture, as `load_revision` would read one back.
/// Going through the real JSON round trip is the point: it is what proves the
/// live and decoded halves of the comparison agree.
function fixture_revision(matrix: SymbolMatrix) -> Revision {
    let report = baml.json.to<PriorReport>(baml.json.parse(baml.json.serialize(matrix)));
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

/// A BAML symbol the export never produced, for asking what happens when the
/// surface gains something.
function fixture_new_baml_symbol(id: string) -> BamlSymbol {
    let none: string[] = [];
    BamlSymbol {
        id: id,
        declared_by: null,
        symbol: ["baml", "invented"],
        display: "baml.invented",
        kind: "function",
        origin: "free",
        generics: none,
        ty: null,
        resolved: null,
        default: null,
        signature: null,
        doc: null,
        references: none,
    }
}

test "a report round-trips into the revision it describes" {
    let matrix = fixture_matrix("sha-r", "5.8.3-test");
    let decoded = fixture_revision(matrix);
    let live = revision_of(matrix);
    // Both halves of the comparison must read the same symbol the same way, or
    // a surface would never go stale.
    for (let symbol in matrix.baml) {
        assert.equal(decoded.texts.get(symbol.id) ?? "", live.texts.get(symbol.id) ?? "");
    }
    assert.is_true(matrix.baml.length() > 0);
}

test "an unchanged symbol keeps its judgement without being asked again" {
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let before = fixture_revision(matrix);
    let outcome = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    assert.equal(outcome.carried, matrix.judgements.length());
    assert.equal(outcome.matrix.judgements.length(), matrix.judgements.length());
}

test "a reworded doc comment invalidates the symbol that carries it" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let judged_id = (matrix.judgements.at(0)?.ts) ?? "";
    let baseline = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    for (let symbol in matrix.ts) {
        if (symbol.id == judged_id) {
            symbol.doc = "reworded";
        }
    }
    // One symbol moved, so one judgement stops carrying. The others are
    // untouched, which is the whole point of asking per symbol.
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, baseline.carried - 1);
}

test "a changed reference invalidates the symbols that name it" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    // Changing a type a signature names must unsettle the symbol, whose own
    // text has not moved at all. `Foo.prototype.clone` returns `Foo`, so it is
    // the one with something to be unsettled by.
    let named: string[] = [];
    for (let symbol in matrix.ts) {
        if (symbol.id == "Foo.prototype.clone") {
            named = symbol.references;
        }
    }
    assert.is_true(named.includes("Foo"));

    let baseline = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    for (let symbol in matrix.ts) {
        if (named.includes(symbol.id)) {
            symbol.doc = "the referenced type changed";
        }
    }
    // Exactly one judgement stops carrying: the one whose signature names the
    // type that moved. Nothing else in the fixture mentions it.
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, baseline.carried - 1);
    assert.equal(outcome.queued, baseline.queued + 1);
}

test "a changed counterpart invalidates the pairing" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    // The counterpart is on the BAML side now: a judgement survives only while
    // every symbol it names reads as it did.
    let paired: string[] = (matrix.judgements.at(0)?.baml) ?? [];
    assert.is_true(paired.length() > 0);
    let baseline = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    for (let symbol in matrix.baml) {
        if (paired.includes(symbol.id)) {
            symbol.doc = "the counterpart changed";
        }
    }
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, baseline.carried - 1);
}

test "a judged absence survives only while the BAML surface could not have gained it" {
    let source = fixture_matrix("sha-a", "5.8.3-test");
    let absent: string[] = [];
    let no_counterpart: string[] = [];
    if let symbol: TsSymbol = source.ts.at(0) {
        source.judgements.push(
            Judgement {
                ts: symbol.id,
                baml: no_counterpart,
                verdict: "none",
                basis: "model",
                confidence: "high",
                reason: "BAML has no equivalent",
                divergence: null,
                rejected: absent,
                verified: true,
                example: null,
            },
        );
    }
    let before = fixture_revision(source);

    // The same BAML surface: the absence still holds.
    let same = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    assert.is_true(same.carried > 0);

    // A BAML symbol that did not exist before could be the very thing the
    // absence denies, so the claim is no longer one this revision can make.
    let grown = fixture_matrix("sha-a", "5.8.3-test");
    grown.baml.push(fixture_new_baml_symbol("V:baml.invented"));
    let after = apply_revision(grown, before);
    assert.is_true(after.carried < same.carried);
}

test "a reworded BAML docstring does not expire an absence" {
    // Only new surface can falsify "there is nothing in BAML that does this".
    // Keying absences on the surface's content would expire every one of them
    // on every run, since we are the team editing that surface.
    let source = fixture_matrix("sha-a", "5.8.3-test");
    let absent: string[] = [];
    let no_counterpart: string[] = [];
    if let symbol: TsSymbol = source.ts.at(0) {
        source.judgements.push(
            Judgement {
                ts: symbol.id,
                baml: no_counterpart,
                verdict: "none",
                basis: "model",
                confidence: "high",
                reason: "BAML has no equivalent",
                divergence: null,
                rejected: absent,
                verified: true,
                example: null,
            },
        );
    }
    let before = fixture_revision(source);
    let same = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);

    let reworded = fixture_matrix("sha-a", "5.8.3-test");
    for (let symbol in reworded.baml) {
        symbol.doc = "every docstring rewritten";
    }
    // The pairings expire — they name BAML symbols whose text moved — but the
    // absence, which names none, survives.
    let outcome = apply_revision(reworded, before);
    assert.is_true(outcome.carried < same.carried);
    let absences = outcome.matrix.judgements.filter((judgement) -> {
        judgement.verdict == "none"
    });
    assert.equal(absences.length(), 1);
}

test "a symbol the previous revision never saw is queued, not carried" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let queued_before = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before).queued;
    // A judgement about a symbol that no longer exists simply finds no home.
    let empty_revision = Revision {
        provenance: before.provenance,
        texts: {},
        baml_ids: before.baml_ids,
        judgements: before.judgements,
    };
    let outcome = apply_revision(matrix, empty_revision);
    assert.equal(outcome.carried, 0);
    assert.equal(outcome.queued, matrix.ts.length());
    assert.is_true(queued_before < matrix.ts.length());
}

test "an unnecessary symbol is neither matched nor missing" {
    // `Array.isArray` is the case this verdict exists for: BAML does not lack
    // it, BAML does not need it. Counting it as either would make the report
    // say something false.
    let matrix = fixture_matrix("sha-u", "5.8.3-test");
    let before = matrix.counts.matched;
    let none: string[] = [];
    let no_counterpart: string[] = [];
    if let symbol: TsSymbol = matrix.ts.at(0) {
        matrix.judgements.push(
            Judgement {
                ts: symbol.id,
                baml: no_counterpart,
                verdict: "unnecessary",
                basis: "model",
                confidence: "high",
                reason: "pattern matching answers this",
                divergence: null,
                rejected: none,
                verified: true,
                example: CodeExample { typescript: "Array.isArray(foo)", baml: "foo is int[]", note: null },
            },
        );
    }
    matrix.counts = recount(matrix);
    assert.equal(matrix.counts.unnecessary, 1);
    // It was recorded against a symbol nothing else speaks for, so the matched
    // tally is untouched and nothing was double counted.
    assert.equal(matrix.counts.matched, before);
    assert.equal(
        matrix.counts.matched
            + matrix.counts.unnecessary
            + matrix.counts.unmatched
            + matrix.counts.unjudged,
        matrix.counts.ts_symbols,
    );
}

test "the strongest verdict about a symbol is the one that counts" {
    // Several judgements can speak for one symbol. Divergence is the finding a
    // reader scans for, so it outranks an exact match; `unnecessary` explains an
    // absence, so it outranks one.
    assert.is_true(judgement_rank("divergent") > judgement_rank("match"));
    assert.is_true(judgement_rank("match") > judgement_rank("unnecessary"));
    assert.is_true(judgement_rank("unnecessary") > judgement_rank("none"));
    // Nothing said at all ranks below everything said, so a symbol nothing has
    // judged never displaces one something has.
    assert.is_true(judgement_rank("none") > judgement_rank(""));
    let empty: Judgement[] = [];
    assert.equal(verdict_of(empty), "");
}

// ── the sessions ────────────────────────────────────────────────────────────

function fixture_finding(
    ts: string,
    verdict: "match" | "divergent" | "unnecessary" | "none",
    baml: string[],
) -> Finding {
    Finding {
        ts: ts,
        verdict: verdict,
        baml: baml,
        confidence: "high",
        reason: "fixture",
        divergence: null,
    }
}

function fixture_answers(findings: Finding[]) -> SessionResult[] {
    let results: SessionResult[] = [];
    results.push(Answer { findings: findings });
    results
}

test "a finding naming a symbol that does not exist is refused" {
    let matrix = fixture_matrix("sha-f", "5.8.3-test");
    let invented: string[] = ["M:baml.Foo.invented"];
    let outcome = apply_findings(
        matrix,
        fixture_answers(
            [
                fixture_finding("Foo.prototype.only", "match", invented),
                fixture_finding("Foo.prototype.invented", "match", ["M:baml.Foo.index_of"]),
            ],
        ),
    );
    // Neither lands: one names a BAML symbol that is not there, the other a
    // TypeScript symbol that is not there.
    assert.equal(outcome.judged, 0);
    assert.equal(outcome.rejected.length(), 2);
}

test "a pairing whose only counterpart was invented is dropped, not recorded as an absence" {
    // The dangerous case. Keeping the finding and emptying its counterpart list
    // would turn a hallucination into a confident "nothing in BAML does this" —
    // a claim about the two libraries, made because a name was wrong.
    let matrix = fixture_matrix("sha-g", "5.8.3-test");
    let invented: string[] = ["V:baml.does_not_exist"];
    let outcome = apply_findings(matrix, fixture_answers([fixture_finding("Foo.prototype.only", "match", invented)]));
    assert.equal(outcome.judged, 0);
    let claims = outcome.matrix.judgements.filter((judgement) -> {
        judgement.ts == "Foo.prototype.only"
    });
    assert.equal(claims.length(), 0);
}

test "a judged absence is recorded, counterpart or not" {
    let matrix = fixture_matrix("sha-h", "5.8.3-test");
    let nothing: string[] = [];
    let outcome = apply_findings(matrix, fixture_answers([fixture_finding("Foo.prototype.only", "none", nothing)]));
    assert.equal(outcome.judged, 1);
    assert.equal(outcome.matrix.counts.unmatched, 1);
}

test "a finding about an already-judged symbol is refused" {
    // `Foo.prototype.indexOf` arrives carried from the fixture's revision.
    let matrix = fixture_matrix("sha-i", "5.8.3-test");
    let outcome = apply_findings(matrix, fixture_answers([fixture_finding("Foo.prototype.indexOf", "none", [])]));
    assert.equal(outcome.judged, 0);
    assert.is_true((outcome.rejected.at(0) ?? "").includes("already judged"));
}

test "a session that does not come back is recorded, not read as finding nothing" {
    let matrix = fixture_matrix("sha-j", "5.8.3-test");
    let results: SessionResult[] = [];
    results.push(PassFailure { pass: "session", subject: "(globals).Foo", reason: "timed out" });
    let outcome = apply_findings(matrix, results);
    assert.equal(outcome.judged, 0);
    assert.equal(outcome.matrix.failures.length(), 1);
    // The symbols it would have judged stay unjudged, which is what the
    // failure entry exists to explain.
    assert.is_true(outcome.matrix.counts.unjudged > 0);
}

test "a session is never asked about more symbols than one answer can hold" {
    let matrix = fixture_matrix("sha-k", "5.8.3-test");
    let requests = session_requests(matrix, null);
    assert.is_true(requests.length() > 0);
    for (let request in requests) {
        assert.is_true(request.ts_lines.length() <= session_chunk());
    }
    // Every unjudged symbol reaches exactly one session: none dropped, none
    // asked about twice.
    let queued = 0;
    for (let request in requests) {
        queued += request.ts_lines.length();
    }
    assert.equal(queued, matrix.counts.unjudged);
}

test "an example that does not compile is caught, and a good one is not" {
    // Goes through the real compiler, because that is the only check worth
    // having: an example is the part of a judgement a reader copies, and a
    // plausible call to a method that does not exist teaches a wrong API more
    // effectively than prose could.
    // Tests run with the project directory as the working directory, and the
    // compiler under test is the same one the run wrapper defaults to.
    let compiler = "../../baml_language/target/debug/baml-cli";
    if (!baml.fs.exists(compiler)) {
        baml.sys.panic(`${compiler} is not built — cargo build -p baml_cli --bin baml-cli`);
    }
    let box = build_toolbox("../..", "../../baml_language/TYPE_SYSTEM.md", compiler, "", "data/scratch");
    let good = #"
        function example() -> string {
            let parts = "a,b,c".split(",");
            parts.join("-")
        }
    "#;
    assert.is_true(check_snippet(box, good) == null);

    let bad = #"
        function example() -> string {
            "a,b,c".splitt(",")
        }
    "#;
    if let error: string = check_snippet(box, bad) {
        // The diagnostic goes back to the model verbatim, so it has to name
        // what is actually wrong.
        assert.is_true(error.includes("splitt"));
    } else {
        baml.sys.panic("a snippet calling a method that does not exist compiled");
    }

    assert.is_true(check_snippet(box, "   ") != null);
}

test "only-matching narrows the sessions without changing them" {
    let matrix = fixture_matrix("sha-o", "5.8.3-test");
    let requests = session_requests(matrix, null);
    let all = only_matching(requests, "");
    assert.equal(all.length(), requests.length());
    let none = only_matching(requests, "nothing-is-called-this");
    assert.equal(none.length(), 0);
    // Case-insensitive substring, so `--only date` finds `(globals).Date`.
    let some = only_matching(requests, "foo");
    assert.is_true(some.length() > 0);
    assert.is_true(some.length() <= requests.length());
}

test "a finding names BAML symbols the way the tools print them" {
    // The bug this exists to prevent, from a real run: the model answered with
    // `baml.time.Instant.now`, which is what `baml describe` prints, and every
    // one of 39 correct answers was refused because the export addresses it as
    // `M:baml.time.Instant.now`. The kind prefix is an implementation detail of
    // the export IR; nothing outside this program should have to know it.
    let matrix = fixture_matrix("sha-n", "5.8.3-test");
    let names = baml_finding_names(matrix);
    assert.equal(resolve_baml_name(names, "baml.Foo.index_of") ?? "", "M:baml.Foo.index_of");
    assert.equal(resolve_baml_name(names, "M:baml.Foo.index_of") ?? "", "M:baml.Foo.index_of");
    assert.equal(resolve_baml_name(names, "Foo.index_of") ?? "", "M:baml.Foo.index_of");
    assert.equal(resolve_baml_name(names, "baml.Foo") ?? "", "T:baml.Foo");
    // Whitespace is the model's, not a different symbol.
    assert.equal(resolve_baml_name(names, "  baml.Foo  ") ?? "", "T:baml.Foo");
    // Still nothing for something that is not there.
    assert.is_true(resolve_baml_name(names, "baml.Foo.invented") == null);
}

test "a finding that names a real symbol by path is applied" {
    let matrix = fixture_matrix("sha-p", "5.8.3-test");
    let path: string[] = ["baml.Foo.sort_helper"];
    let outcome = apply_findings(matrix, fixture_answers([fixture_finding("Foo.prototype.only", "match", path)]));
    assert.equal(outcome.judged, 1);
    assert.equal(outcome.rejected.length(), 0);
    // Stored as the id, whatever spelling arrived: the report keys on ids, and
    // a judgement holding a display would not survive the next revision.
    let applied = outcome.matrix.judgements.filter((judgement) -> {
        judgement.ts == "Foo.prototype.only"
    });
    if let judgement: Judgement = applied.at(0) {
        assert.equal(judgement.baml, ["M:baml.Foo.sort_helper"]);
    } else {
        baml.sys.panic("the finding was not applied");
    }
}

test "a session knows exactly which symbols it still owes an answer for" {
    // A real run answered for 51 of 52 symbols and was accepted, leaving
    // `Date.prototype.setHours` silently unjudged — which reads identically to
    // never having asked. The retry turn names what is missing rather than
    // counting it, so the model does not have to re-derive which one it skipped.
    let matrix = fixture_matrix("sha-r2", "5.8.3-test");
    let requests = session_requests(matrix, null);
    if let request: SessionRequest = requests.at(0) {
        assert.equal(request.ts_ids.length(), request.ts_lines.length());
        // Nothing covered yet: every symbol is owed.
        let nothing: map<string, bool> = {};
        let no_complaints: string[] = [];
        let all_owed = retry_turn(request, nothing, no_complaints);
        for (let id in request.ts_ids) {
            assert.is_true(all_owed.includes(id));
        }

        // One outstanding: it is named, and the ones already answered are not
        // asked for again.
        let covered: map<string, bool> = {};
        let owed = request.ts_ids.at(0) ?? "";
        for (let id in request.ts_ids) {
            if (id != owed) {
                let _ = covered.set(id, true);
            }
        }
        let one = retry_turn(request, covered, no_complaints);
        assert.is_true(one.includes(owed));
        assert.is_true(one.includes("do not repeat it"));
    } else {
        baml.sys.panic("no sessions were planned");
    }
}

test "an answer keeps its finding when only the example is bad" {
    // Running out of attempts costs the example, never the judgement.
    let finding = Finding {
        ts: "Foo.prototype.only",
        verdict: "match",
        baml: ["M:baml.Foo.index_of"],
        confidence: "high",
        reason: "fixture",
        divergence: null,
        example: CodeExample { typescript: "foo.only()", baml: "this does not compile", note: null },
    };
    let names: map<string, string> = { "M:baml.Foo.index_of": "M:baml.Foo.index_of" };
    if let stripped: Finding = without_faults(finding, names) {
        assert.is_true(stripped.example == null);
        assert.equal(stripped.verdict, finding.verdict);
        assert.equal(stripped.baml, finding.baml);
        assert.equal(stripped.reason, finding.reason);
    } else {
        baml.sys.panic("a finding with a real counterpart was dropped");
    }

    // But a pairing whose every counterpart was invented has nothing left to
    // salvage: keeping it with an empty list would turn a hallucination into a
    // confident "nothing in BAML does this".
    let invented = Finding {
        ts: "Foo.prototype.only",
        verdict: "match",
        baml: ["baml.does_not_exist"],
        confidence: "high",
        reason: "fixture",
        divergence: null,
        example: null,
    };
    assert.is_true(without_faults(invented, names) == null);
}

/// A toolbox that can answer nothing. Enough for the checks that run before
/// any tool is reached — a finding with no example never touches the compiler.
function fixture_toolbox() -> Toolbox {
    let headings: string[] = [];
    let bodies: map<string, string> = {};
    Toolbox {
        skill: "",
        doc_headings: headings,
        doc_bodies: bodies,
        repo_root: ".",
        scratch_root: ".",
        baml_bin: "baml-cli-that-is-never-run",
    }
}

/// Built fresh per call, not aliased: class values are references, so mutating
/// a `let` bound to one of these would edit the finding it was copied from.
function fixture_divergence(
    verdict: "match" | "divergent" | "unnecessary" | "none",
    divergence: string?,
) -> Finding {
    Finding {
        ts: "Foo.prototype.only",
        verdict: verdict,
        baml: ["M:baml.Foo.index_of"],
        confidence: "high",
        reason: "fixture",
        divergence: divergence,
        example: null,
    }
}

test "a pairing verdict that names nothing is sent back" {
    // `apply_findings` already refused these, but it runs after the session has
    // ended: the symbol was marked covered, no retry was spent, and it came out
    // unjudged with the reason recorded where the session could not read it.
    let box = fixture_toolbox();
    let names: map<string, string> = { "M:baml.Foo.index_of": "M:baml.Foo.index_of" };
    let nothing: string[] = [];

    let bare_match = Finding {
        ts: "Foo.prototype.only",
        verdict: "match",
        baml: nothing,
        confidence: "high",
        reason: "fixture",
        divergence: null,
        example: null,
    };
    if let complaint: string = finding_fault(box, bare_match, names) {
        assert.is_true(complaint.includes("named no BAML symbol"));
    } else {
        baml.sys.panic("a match naming nothing was accepted");
    }

    let bare_divergent = Finding {
        ts: "Foo.prototype.only",
        verdict: "divergent",
        baml: nothing,
        confidence: "high",
        reason: "fixture",
        divergence: "they differ",
        example: null,
    };
    if let complaint: string = finding_fault(box, bare_divergent, names) {
        assert.is_true(complaint.includes("named no BAML symbol"));
    } else {
        baml.sys.panic("a divergent naming nothing was accepted");
    }

    // The verdicts that legitimately name nothing are untouched.
    let absent = Finding {
        ts: "Foo.prototype.only",
        verdict: "none",
        baml: nothing,
        confidence: "high",
        reason: "fixture",
        divergence: null,
        example: null,
    };
    assert.is_true(finding_fault(box, absent, names) == null);
}

test "freshness follows references all the way, not one step" {
    // A judgement naming A carried when A referenced B and B was built from a
    // changed C. One hop short of what the module header already promised.
    let seed = fixture_revision(fixture_matrix("sha-t", "5.8.3-test"));
    let before = Revision {
        provenance: seed.provenance,
        texts: { "M:baml.a": "a", "T:baml.b": "b", "T:baml.c": "c" },
        baml_ids: seed.baml_ids,
        judgements: {},
    };
    let references: map<string, string[]> = { "M:baml.a": ["T:baml.b"], "T:baml.b": ["T:baml.c"] };
    // C two hops out has been retyped.
    let now: map<string, string> = { "M:baml.a": "a", "T:baml.b": "b", "T:baml.c": "changed" };
    assert.is_true(!is_fresh_transitively("M:baml.a", now, before, references));

    // Nothing moved: it carries.
    let still: map<string, string> = { "M:baml.a": "a", "T:baml.b": "b", "T:baml.c": "c" };
    assert.is_true(is_fresh_transitively("M:baml.a", still, before, references));

    // A cycle terminates rather than spinning.
    let cyclic: map<string, string[]> = { "M:baml.a": ["T:baml.b"], "T:baml.b": ["M:baml.a"] };
    assert.is_true(is_fresh_transitively("M:baml.a", still, before, cyclic));
}

test "a divergent verdict must say how they diverge" {
    // Thirteen shipped without one. `divergent` is not a stronger `match`; the
    // caveat is the entire difference between them, and a striped swatch whose
    // tooltip is empty tells the reader nothing they can act on.
    let box = fixture_toolbox();
    let names: map<string, string> = { "M:baml.Foo.index_of": "M:baml.Foo.index_of" };

    if let complaint: string = finding_fault(box, fixture_divergence("divergent", null), names) {
        assert.is_true(complaint.includes("did not say how"));
    } else {
        baml.sys.panic("a divergent finding with no divergence was accepted");
    }
    // Whitespace is not a difference either.
    assert.is_true(finding_fault(box, fixture_divergence("divergent", "   "), names) != null);

    // And the converse: a difference written under a verdict that claims none.
    let contradictory = fixture_divergence("match", "the argument order is reversed");
    if let complaint: string = finding_fault(box, contradictory, names) {
        assert.is_true(complaint.includes("the verdict is match"));
    } else {
        baml.sys.panic("a match carrying a divergence was accepted");
    }

    // Stated, it passes.
    let stated = fixture_divergence("divergent", "BAML returns an optional rather than -1");
    assert.is_true(finding_fault(box, stated, names) == null);
}

test "a long tool answer is cut, and says where" {
    // Every round's results stay in the transcript for every later round, so an
    // unbounded tool answer is a transcript that grows unboundedly. Silent
    // truncation would be the worse failure: a model would read a class's
    // members as complete and conclude the one it needed is absent.
    let short = "already small";
    assert.equal(clip(short, 100), short);
    let long = "x".repeat(200);
    let cut = clip(long, 50);
    assert.is_true(cut.length() < long.length());
    assert.is_true(cut.includes("truncated at 50 characters"));
    // Exactly at the budget is not truncated.
    assert.equal(clip("y".repeat(50), 50).length(), 50);
}

test "the last round cannot ask for another lookup" {
    // Three sessions in a real run were told the lookup budget was spent, kept
    // investigating anyway, and died having judged nothing. Advice can be
    // declined; a return type cannot — so the final call answers and offers no
    // other shape.
    //
    // Asserted through the rendered output format because that *is* the
    // mechanism: it is what the model is shown. This has already been lost once
    // to a careless edit, leaving a comment describing a function that no
    // longer existed, so it is worth a test that costs nothing.
    let step = baml.prompt.render_output_format(baml.prompt.get_return_type("session_step"));
    let last = baml.prompt.render_output_format(baml.prompt.get_return_type("session_final"));
    assert.is_true(step.includes("any of these schemas"));
    assert.is_true(step.includes("lookups"));
    assert.is_true(!last.includes("any of these schemas"));
    assert.is_true(last.includes("findings"));
    assert.is_true(!last.includes("lookups"));
}

test "a judgement goes stale when a type its counterpart names changes" {
    // The case revision.baml's own header describes and the code did not do:
    // `read(path) -> baml.fs.DirEntry[]` means something new the moment
    // `DirEntry` gains a field. `is_fresh` ran on the TypeScript side only, so
    // retyping a field left every judgement about reading a directory carried,
    // shipping reasoning written against the old type.
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let baseline = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);

    // `M:baml.Foo.clone` returns `baml.Foo`, so it names a type. Nothing about
    // the method itself moves — only the type it hands back.
    let named: string[] = [];
    for (let symbol in matrix.baml) {
        if (symbol.id == "M:baml.Foo.clone") {
            named = symbol.references;
        }
    }
    assert.is_true(named.includes("T:baml.Foo"));
    for (let symbol in matrix.baml) {
        if (symbol.id == "T:baml.Foo") {
            symbol.doc = "the type gained a field";
        }
    }

    let outcome = apply_revision(matrix, before);
    let now = current_texts(matrix);
    // The methods themselves did not move — only the type they hand back and
    // take as a receiver. That is the whole point: their own text compares
    // equal, so a check that stopped at the named symbol would carry them.
    assert.equal(now.get("M:baml.Foo.clone") ?? "", before.texts.get("M:baml.Foo.clone") ?? "");
    assert.is_true((now.get("T:baml.Foo") ?? "") != (before.texts.get("T:baml.Foo") ?? ""));
    assert.is_true(baseline.carried > 0);
    assert.equal(outcome.carried, 0);
}

test "the field separator is one character that content cannot contain" {
    // It was `\u{1f}`, which BAML does not parse as an escape — so the
    // separator was six printable characters that a docstring could contain,
    // and two symbols differing only in where a field boundary fell could
    // compare equal.
    assert.equal(field_separator().length(), 1);
    assert.is_true(!field_separator().is_graphic());
}

test "a pairing that names nothing is refused, whatever the verdict says" {
    // The shipped report had 11 of these: `match` and `divergent` findings with
    // an empty counterpart list, because the guard only fired when a name had
    // been *offered and refused*. They inflated `matched`, rendered a "BAML"
    // section with nothing under it, and — since the freshness rule read the
    // empty list as an absence — were cached against a rule for a different
    // claim entirely.
    let matrix = fixture_matrix("sha-e", "5.8.3-test");
    let nothing: string[] = [];
    let outcome = apply_findings(
        matrix,
        fixture_answers(
            [
                fixture_finding("Foo.prototype.only", "match", nothing),
                fixture_finding("Foo.prototype.sortStuff", "divergent", nothing),
            ],
        ),
    );
    assert.equal(outcome.judged, 0);
    assert.equal(outcome.rejected.length(), 2);
    assert.is_true((outcome.rejected.at(0) ?? "").includes("names none"));

    // An absence naming nothing is exactly right, and still lands.
    let absence = apply_findings(
        fixture_matrix("sha-e2", "5.8.3-test"),
        fixture_answers([fixture_finding("Foo.prototype.only", "none", nothing)]),
    );
    assert.equal(absence.judged, 1);
}

test "the markdown summary and its table agree about what is claimed" {
    // They disagreed by eight rows in the shipped report: `recount` counts a
    // symbol as named whatever the verdict, while the renderer counted only
    // `match`/`divergent`. Eight symbols were listed under "BAML symbols no
    // judgement names" in the very table the summary line introduces.
    let matrix = fixture_matrix("sha-md", "5.8.3-test");
    let unnecessary_only: string[] = ["M:baml.Foo.sort_helper"];
    matrix.judgements.push(
        Judgement {
            ts: "Foo.prototype.only",
            baml: unnecessary_only,
            verdict: "unnecessary",
            basis: "model",
            confidence: "high",
            reason: "the language answers this",
            divergence: null,
            rejected: [],
            verified: false,
            example: null,
        },
    );
    matrix.counts = recount(matrix);

    let markdown = render_matrix_markdown(matrix);
    // The one named only by an `unnecessary` judgement is claimed, so it is
    // absent from the table of symbols nothing names.
    let table = markdown.split("## BAML symbols no judgement names").at(1) ?? "";
    assert.is_true(!table.includes("Foo.sort_helper"));
    assert.is_true(
        markdown.includes(`${matrix.counts.baml_symbols - matrix.counts.baml_unclaimed} of`),
    );
}

test "an interstitial is not mistaken for the skill" {
    // A captive portal or CDN error page returns 200 with HTML, and
    // `response.ok()` is true for all of them. Accepting one told every session
    // that BAML is that HTML, and overwrote the good cached copy on the way.
    assert.is_true(
        !looks_like_skill("<!DOCTYPE html><html><body>Sign in to continue</body></html>"),
    );
    assert.is_true(!looks_like_skill(""));
    assert.is_true(!looks_like_skill("baml"));
    // Long enough, not markup, and about BAML.
    let real = `---\nname: baml-core\n---\n\n# baml\n\nBAML is a statically-typed language. ${"filler ".repeat(400)}`;
    assert.is_true(looks_like_skill(real));
}

test "the concurrency cap falls back when nothing was asked for" {
    assert.equal(effective_limit(0), call_limit());
    assert.equal(effective_limit(-1), call_limit());
    assert.equal(effective_limit(24), 24);
}
