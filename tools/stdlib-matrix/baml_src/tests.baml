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

test "match keys meet across snake_case and camelCase" {
    assert.equal(match_key("to_upper_case"), "touppercase");
    assert.equal(match_key("toUpperCase"), "touppercase");
    assert.equal(match_key("index_of"), match_key("indexOf"));
}

test "id helpers split kind prefixes and path segments" {
    assert
        .equal(
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
        assert.is_true(baml.deep_equals(container.symbol, ["(globals)", "Foo"]));
    } else {
        baml.sys.panic("no TS symbols");
    }
    if let index_of: TsSymbol = symbols.at(1) {
        assert.equal(index_of.display, "Foo.prototype.indexOf");
        assert.equal(index_of.origin, "method");
        assert.is_true(baml.deep_equals(index_of.symbol, ["(globals)", "Foo", "indexOf"]));
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

test "the symbol matrix relates both sides without privileging either" {
    let matrix = build_symbol_matrix(
        parse_export_text(fixture_export_text()),
        baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()),
        "sha-fixture",
    );
    assert.equal(matrix.counts.baml_symbols, 9);
    // `Foo`, its four members, and `parseFixture`.
    assert.equal(matrix.counts.ts_symbols, 6);

    // index_of ↔ indexOf is deterministic name agreement, and both endpoints
    // address real symbols by id.
    assert.is_true(matrix.judgements.length() >= 1);
    let baml_ids: map<string, bool> = {};
    for (let symbol in matrix.baml) {
        let _ = baml_ids.set(symbol.id, true);
    }
    let ts_ids: map<string, bool> = {};
    for (let symbol in matrix.ts) {
        let _ = ts_ids.set(symbol.id, true);
    }
    if let first: Judgement = matrix.judgements.at(0) {
        assert.equal(first.basis, "name");
        assert.equal(first.verdict, "match");
        // A name match arrives unverified: it is a proposal like any other.
        assert.is_true(!first.verified);
        assert.is_true(ts_ids.has(first.ts));
        assert.is_true(first.baml.length() > 0);
        for (let id in first.baml) {
            assert.is_true(baml_ids.has(id));
        }
    } else {
        baml.sys.panic("no judgement recorded for index_of");
    }

    // A symbol nothing has looked at carries no judgement, which is a different
    // state from one judged to have no counterpart.
    assert.is_true(matrix.counts.unjudged > 0);
    assert.equal(matrix.counts.unmatched, 0);
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

function fixture_matrix(sha: string, ts_version: string) -> SymbolMatrix {
    let matrix = build_symbol_matrix(
        parse_export_text(fixture_export_text()),
        baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()),
        sha,
    );
    matrix.provenance.typescript_version = ts_version;
    matrix
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
    assert.is_true(baml.deep_equals(type_names("baml.fs.DirEntry[]"), ["baml.fs.DirEntry"]));
    assert
        .is_true(baml.deep_equals(type_names("map<string, unknown>"), ["map", "string", "unknown"]));
    // A dotted path is one name, and the punctuation around a projection does
    // not glue onto either side of it.
    assert
        .is_true(
            baml.deep_equals(
                type_names("(Self as baml.Comparable).CompareError"),
                ["Self", "as", "baml.Comparable", "CompareError"],
            ),
        );
    // String-literal contents survive as names that resolve to nothing.
    assert.is_true(baml.deep_equals(type_names("\"r\" | \"w+\""), ["r", "w"]));
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
        assert.is_true(baml.deep_equals(index_of.references, ["T:baml.Foo"]));
    } else {
        baml.sys.panic("Foo.index_of missing");
    }

    // A type declaration names nothing by existing.
    if let foo: BamlSymbol = by_display.get("Foo") {
        let none: string[] = [];
        assert.is_true(baml.deep_equals(foo.references, none));
    } else {
        baml.sys.panic("Foo missing");
    }

    // `compare` throws `CompareError`, which nothing in this package declares:
    // an unresolvable name is dropped rather than guessed at.
    let compares = symbols.filter((symbol) -> {
        symbol.origin == "impl_method" && symbol.display.includes("compare")
    });
    if let compare: BamlSymbol = compares.at(0) {
        assert.is_true(baml.deep_equals(compare.references, ["T:baml.Foo"]));
    } else {
        baml.sys.panic("the impl's compare is missing");
    }
}

test "a bare name resolves through its own scope, never across the surface" {
    let index = NameIndex { types: {}, all: {} };
    let _ = index_put(index.types, "baml.iter.Chain", "T:baml.iter.Chain");
    let _ = index_put(index.types, "baml.toml.Item", "T:baml.toml.Item");

    // Written inside `baml.iter`, `Chain` finds its neighbour.
    assert
        .equal(
            resolve_reference(index, "Chain", ["baml", "iter", "ArrayIterator"]) ?? "",
            "T:baml.iter.Chain",
        );
    // `Item` inside `baml.iter` is an associated type, not `baml.toml.Item`.
    // Matching bare names across the whole surface is what would pair them.
    assert.is_true(resolve_reference(index, "Item", ["baml", "iter"]) == null);
    // Qualified, it resolves from anywhere.
    assert
        .equal(
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
    assert
        .is_true(
            baml.deep_equals(
                ids,
                [
                    "X:baml.impl[baml.Comparable for baml.Foo]::compare",
                    "X:baml.impl[baml.Comparable for baml.Foo]::index_of",
                ],
            ),
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
        source
            .judgements
            .push(
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
        source
            .judgements
            .push(
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

test "the concurrency cap falls back when nothing was asked for" {
    assert.equal(effective_limit(0), call_limit());
    assert.equal(effective_limit(-1), call_limit());
    assert.equal(effective_limit(24), 24);
}
