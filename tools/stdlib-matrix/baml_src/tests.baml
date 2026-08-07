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
          "format_version": 1,
          "typescript_version": "5.8.3-test",
          "types_node_version": null,
          "containers": [
            {
              "name": "Foo",
              "kind": "interface",
              "members": [
                {"name": "indexOf", "static": false, "signatures": ["indexOf(x: number): number"], "doc": "Finds x.", "since": "es5"},
                {"name": "only", "static": false, "signatures": ["only(): void"], "doc": null, "since": "es5"},
                {"name": "sortStuff", "static": false, "signatures": ["sortStuff(): void"], "doc": null, "since": "es2015"}
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
    let container = TsContainer { name: "Number", kind: "interface", members: [] };
    let constant = TsMember {
        name: "MAX_SAFE_INTEGER",
        static: true,
        signatures: ["readonly MAX_SAFE_INTEGER: number"],
        doc: null,
        since: "es2015",
    };
    // On the constructor side, but a constant. Comparing it against BAML's
    // methods rather than its fields is the wrong question to ask.
    assert.equal(ts_origin_of(container, constant), "property");
    let factory = TsMember {
        name: "isInteger",
        static: true,
        signatures: ["isInteger(number: unknown): boolean"],
        doc: null,
        since: "es2015",
    };
    assert.equal(ts_origin_of(container, factory), "static_method");
}

test "TypeScript symbols separate callables from properties" {
    let symbols = extract_ts_symbols(baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()));
    assert.equal(symbols.length(), 3);
    if let index_of: TsSymbol = symbols.at(0) {
        assert.equal(index_of.display, "Foo.indexOf");
        assert.equal(index_of.origin, "method");
    } else {
        baml.sys.panic("no TS symbols");
    }
}

test "the symbol matrix relates both sides without privileging either" {
    let matrix = build_symbol_matrix(
        parse_export_text(fixture_export_text()),
        baml.json.from_string<TsSurfaceDoc>(fixture_ts_text()),
        "sha-fixture",
    );
    assert.equal(matrix.counts.baml_symbols, 8);
    assert.equal(matrix.counts.ts_symbols, 3);

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
        assert.is_true(baml_ids.has(first.baml));
        assert.is_true(ts_ids.has(first.ts ?? ""));
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
    assert.is_true(markdown.includes("## BAML surface"));
    assert.is_true(markdown.includes("TypeScript symbols with no BAML counterpart"));
    // An impl method groups under the impl it arrives through.
    assert.is_true(markdown.includes("baml.Foo as baml.Comparable"));
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
    let instance = TsContainer { name: "Uint8Array", kind: "interface", members: [] };
    let member = TsMember {
        name: "BYTES_PER_ELEMENT",
        static: false,
        signatures: ["readonly BYTES_PER_ELEMENT: number"],
        doc: null,
        since: "es5",
    };
    assert.equal(ts_id_of(instance, member, "property"), "Uint8Array.prototype.BYTES_PER_ELEMENT");
    // The same name on the constructor side. `static` decides, not `origin`:
    // the constructor holds constants as well as methods and both are reached
    // on it, so deciding from `origin` put a constant behind `prototype` and
    // gave the two sides one address.
    let constructor_side = TsMember {
        name: "BYTES_PER_ELEMENT",
        static: true,
        signatures: ["readonly BYTES_PER_ELEMENT: number"],
        doc: null,
        since: "es5",
    };
    assert.equal(ts_id_of(instance, constructor_side, "property"), "Uint8Array.BYTES_PER_ELEMENT");
    // A free function has no receiver to be reached through.
    let globals = TsContainer { name: "(globals)", kind: "functions", members: [] };
    assert.equal(ts_id_of(globals, member, "free"), "(globals).BYTES_PER_ELEMENT");
}

// ── carrying a revision forward ─────────────────────────────────────────────

/// A previous revision of the fixture, as `load_revision` would read one back.
/// Going through the real JSON round trip is the point: it is what proves the
/// live and decoded halves of the comparison agree.
function fixture_revision(matrix: SymbolMatrix) -> Revision {
    let report = baml.json.to<PriorReport>(baml.json.parse(baml.json.serialize(matrix)));
    let texts: map<string, string> = {};
    for (let symbol in report.baml) {
        let _ = texts.set(symbol.id, prior_symbol_text(symbol));
    }
    for (let symbol in report.ts) {
        let _ = texts.set(symbol.id, ts_symbol_text(symbol));
    }
    let judgements: map<string, Judgement[]> = {};
    for (let judgement in report.judgements) {
        let bucket: Judgement[] = judgements.get(judgement.baml) ?? [];
        bucket.push(judgement);
        let _ = judgements.set(judgement.baml, bucket);
    }
    Revision { provenance: report.provenance, texts: texts, judgements: judgements }
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
    let judged_id = (matrix.judgements.at(0)?.baml) ?? "";
    for (let symbol in matrix.baml) {
        if (symbol.id == judged_id) {
            symbol.doc = "reworded";
        }
    }
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, 0);
}

test "a changed reference invalidates the symbols that name it" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    // `Foo.index_of` names `baml.Foo`; changing the type it names must unsettle
    // the method, whose own text has not moved at all.
    let judged_id = (matrix.judgements.at(0)?.baml) ?? "";
    let named: string[] = [];
    for (let symbol in matrix.baml) {
        if (symbol.id == judged_id) {
            named = symbol.references;
        }
    }
    assert.is_true(named.length() > 0);
    for (let symbol in matrix.baml) {
        if (named.includes(symbol.id)) {
            symbol.doc = "the referenced type changed";
        }
    }
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, 0);
}

test "a changed counterpart invalidates the pairing" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let paired = (matrix.judgements.at(0)?.ts) ?? "";
    for (let symbol in matrix.ts) {
        if (symbol.id == paired) {
            symbol.signature = "indexOf(x: number, from: number): number";
        }
    }
    let outcome = apply_revision(matrix, before);
    assert.equal(outcome.carried, 0);
}

test "a judged absence survives only while the surface it was judged against does" {
    let source = fixture_matrix("sha-a", "5.8.3-test");
    let absent: string[] = [];
    if let symbol: BamlSymbol = source.baml.at(0) {
        source
            .judgements
            .push(
                Judgement {
                    baml: symbol.id,
                    ts: null,
                    verdict: "none",
                    basis: "model",
                    confidence: "high",
                    reason: "TypeScript has no equivalent",
                    divergence: null,
                    rejected: absent,
                    verified: true,
                },
            );
    }
    let before = fixture_revision(source);

    // The same TypeScript release: the absence still holds.
    let same = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    assert.is_true(same.carried > 0);

    // A new TypeScript release could have added the very thing the absence
    // denies, so the claim is no longer one this revision can make.
    let bumped = apply_revision(fixture_matrix("sha-a", "5.9.0-test"), before);
    assert.is_true(bumped.carried < same.carried);
}

test "a symbol the previous revision never saw is queued, not carried" {
    let before = fixture_revision(fixture_matrix("sha-a", "5.8.3-test"));
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let queued_before = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before).queued;
    // A judgement about a symbol that no longer exists simply finds no home.
    let empty_revision = Revision { provenance: before.provenance, texts: {}, judgements: before.judgements };
    let outcome = apply_revision(matrix, empty_revision);
    assert.equal(outcome.carried, 0);
    assert.equal(outcome.queued, matrix.baml.length());
    assert.is_true(queued_before < matrix.baml.length());
}

// ── candidate pools ─────────────────────────────────────────────────────────

test "the correspondence table walks outward until it has something to say" {
    // A type inherits the answer for the namespace it lives in.
    assert.is_true(baml.deep_equals(pool_containers("baml.time.ZonedDateTime"), ["Date"]));
    assert.is_true(baml.deep_equals(pool_containers("baml.time"), ["Date"]));
    // A type with its own entry does not.
    assert.is_true(baml.deep_equals(pool_containers("baml.http.Response"), ["Response"]));
    // An empty entry is an answer, and the walk stops there rather than
    // inheriting something from further out.
    let none: string[] = [];
    assert.is_true(baml.deep_equals(pool_containers("baml.csv.CsvReader"), none));
    assert.is_true(baml.deep_equals(pool_containers("baml.nowhere.At.All"), none));
}

test "a companion class is keyed by the type it backs" {
    let symbols = extract_symbols(parse_export_text(fixture_export_text()));
    for (let symbol in symbols) {
        if (symbol.display == "Foo.index_of") {
            assert.equal(baml_owner_display(symbol), "baml.Foo");
        }
    }
    // An impl's receiver reduces to the type that owns it, generics and all.
    assert.equal(normalize_impl_base("float[]"), "T[]");
    assert.equal(normalize_impl_base("map<string, V>"), "map<K, V>");
    assert.equal(normalize_impl_base("baml.String"), "string");
    assert.equal(normalize_impl_base("baml.iter.Chain<T, E, E2>"), "baml.iter.Chain");
}

test "the lexical shortlist ignores names too short to mean anything" {
    assert.is_true(names_share_a_stem("to_string", "toString"));
    assert.is_true(names_share_a_stem("parse", "parseInt"));
    // `get` agrees with `get` across a dozen unrelated containers.
    assert.is_true(!names_share_a_stem("get", "get"));
    assert.is_true(!names_share_a_stem("at", "at"));
    assert.is_true(!names_share_a_stem("split", "join"));
}

test "a pool keeps its container's members whatever else is claimed" {
    let matrix = fixture_matrix("sha-p", "5.8.3-test");
    let pools = build_pools(matrix, matrix.baml);
    let by_owner: map<string, CandidatePool> = {};
    for (let pool in pools) {
        let _ = by_owner.set(pool.owner, pool);
    }

    // `baml.Foo` has no entry in the table, so its pool is entirely lexical:
    // `index_of` finds `indexOf`, `sort_helper` finds `sortStuff`.
    if let foo: CandidatePool = by_owner.get("baml.Foo") {
        assert.is_true(foo.ts_ids.includes("Foo.prototype.indexOf"));
        assert.equal(foo.dropped, 0);
    } else {
        baml.sys.panic("no pool for baml.Foo");
    }

    // A symbol another judgement already claims stays in the pool. The two
    // libraries are not in bijection, and hiding a claimed symbol from the next
    // caller is how the right answer goes missing.
    let claimed = (matrix.judgements.at(0)?.ts) ?? "";
    assert.is_true(claimed.length() > 0);
    let found = false;
    for (let pool in pools) {
        if (pool.ts_ids.includes(claimed)) {
            found = true;
        }
    }
    assert.is_true(found);
}

test "an owner with no plausible counterpart gets an empty pool, not a guess" {
    let matrix = fixture_matrix("sha-p", "5.8.3-test");
    // No container, and nothing whose name shares a stem with `only`.
    let members = matrix.baml.filter((symbol) -> {
        symbol.display == "helper_fn"
    });
    let pool = build_pool("baml.llm", members, matrix.ts);
    assert.equal(pool.containers.length(), 0);
    assert.equal(pool.ts_ids.length(), 0);
}

// ── the passes ──────────────────────────────────────────────────────────────

/// A pass result carrying exactly the proposals given. Standing in for a call
/// lets every fold be tested without one — the folds are where claims are
/// accepted or refused, and that is the part worth pinning.
function fixture_proposals(proposals: Proposal[]) -> PassResult[] {
    let results: PassResult[] = [];
    results.push(ProposeVerdict { proposals: proposals });
    results
}

function fixture_proposal(
    baml_id: string,
    verdict: "match" | "divergent" | "none" | "bless",
    ts_id: string?,
) -> Proposal {
    Proposal {
        baml: baml_id,
        verdict: verdict,
        ts: ts_id,
        confidence: "high",
        reason: "fixture",
        divergence: null,
    }
}

test "a proposal naming a symbol that was never offered is refused" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let outcome = apply_proposals(
        matrix,
        fixture_proposals(
            [
                fixture_proposal("M:baml.Foo.invented", "match", "Foo.prototype.indexOf"),
                fixture_proposal("M:baml.Foo.from_parts", "match", "Foo.prototype.invented"),
            ],
        ),
        null,
    );
    assert.equal(outcome.proposed, 0);
    assert.equal(outcome.rejected.length(), 2);
    assert.is_true((outcome.rejected.at(0) ?? "").includes("not a BAML symbol"));
    assert.is_true((outcome.rejected.at(1) ?? "").includes("not a TypeScript symbol"));
}

test "a proposal about an already-judged symbol is refused" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let settled = (matrix.judgements.at(0)?.baml) ?? "";
    let outcome = apply_proposals(matrix, fixture_proposals([fixture_proposal(settled, "match", "Foo.prototype.only")]), null);
    assert.equal(outcome.proposed, 0);
    assert.is_true((outcome.rejected.at(0) ?? "").includes("already judged"));
}

test "a judged absence is recorded, counterpart or not" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let outcome = apply_proposals(matrix, fixture_proposals([fixture_proposal("M:baml.Foo.from_parts", "none", null)]), null);
    assert.equal(outcome.proposed, 1);
    let recorded = outcome.matrix.judgements.filter((judgement) -> {
        judgement.baml == "M:baml.Foo.from_parts"
    });
    if let judgement: Judgement = recorded.at(0) {
        assert.equal(judgement.verdict, "none");
        assert.is_true(judgement.ts == null);
        assert.equal(judgement.basis, "model");
    } else {
        baml.sys.panic("the absence was not recorded");
    }
    // Judged absent is a different state from never looked at.
    assert.equal(outcome.matrix.counts.unmatched, 1);
}

test "blessing takes the previous verdict whole and marks it examined" {
    let source = fixture_matrix("sha-x", "5.8.3-test");
    let before = fixture_revision(source);
    let settled = (source.judgements.at(0)?.baml) ?? "";
    let paired = (source.judgements.at(0)?.ts) ?? "";

    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    matrix.judgements = [];
    let outcome = apply_proposals(matrix, fixture_proposals([fixture_proposal(settled, "bless", null)]), before);
    assert.equal(outcome.proposed, 1);
    if let judgement: Judgement = outcome.matrix.judgements.at(0) {
        assert.equal(judgement.ts ?? "", paired);
        assert.equal(judgement.basis, "model");
        // A model looked at it again, which is what blessing means.
        assert.is_true(judgement.verified);
    } else {
        baml.sys.panic("nothing was blessed");
    }
}

test "blessing with nothing to bless is refused" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    matrix.judgements = [];
    let outcome = apply_proposals(matrix, fixture_proposals([fixture_proposal("M:baml.Foo.from_parts", "bless", null)]), null);
    assert.equal(outcome.proposed, 0);
    assert.is_true((outcome.rejected.at(0) ?? "").includes("no previous verdict"));
}

test "a call that does not come back is recorded, not read as finding nothing" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let results: PassResult[] = [];
    results.push(PassFailure { pass: "propose", subject: "baml.Foo", reason: "timeout" });
    let outcome = apply_proposals(matrix, results, null);
    assert.equal(outcome.proposed, 0);
    assert.equal(outcome.matrix.failures.length(), 1);
    if let failure: PassFailure = outcome.matrix.failures.at(0) {
        assert.equal(failure.subject, "baml.Foo");
    } else {
        baml.sys.panic("the failure was not recorded");
    }
}

test "a refuted pairing becomes an absence that remembers what it refused" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let settled = (matrix.judgements.at(0)?.baml) ?? "";
    let paired = (matrix.judgements.at(0)?.ts) ?? "";
    let results: VerifyResult[] = [];
    results
        .push(
            VerifyVerdict {
                rulings: [
                    Ruling {
                        baml: settled,
                        ts: paired,
                        upheld: false,
                        reason: "one throws where the other returns null",
                        divergence: null,
                    },
                ],
            },
        );
    let outcome = apply_rulings(matrix, results);
    assert.equal(outcome.refuted, 1);
    assert.equal(outcome.upheld, 0);
    let recorded = outcome.matrix.judgements.filter((judgement) -> {
        judgement.baml == settled
    });
    if let judgement: Judgement = recorded.at(0) {
        assert.equal(judgement.verdict, "none");
        assert.is_true(judgement.ts == null);
        // The refused counterpart is kept so the next run does not offer it again.
        assert.is_true(judgement.rejected.includes(paired));
        assert.is_true(judgement.verified);
    } else {
        baml.sys.panic("the refutation was not recorded");
    }
}

test "an upheld pairing that differs is recorded as divergent" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let settled = (matrix.judgements.at(0)?.baml) ?? "";
    let paired = (matrix.judgements.at(0)?.ts) ?? "";
    let results: VerifyResult[] = [];
    results
        .push(
            VerifyVerdict {
                rulings: [
                    Ruling {
                        baml: settled,
                        ts: paired,
                        upheld: true,
                        reason: "same operation",
                        divergence: "BAML returns null where TypeScript returns -1",
                    },
                ],
            },
        );
    let outcome = apply_rulings(matrix, results);
    assert.equal(outcome.upheld, 1);
    let recorded = outcome.matrix.judgements.filter((judgement) -> {
        judgement.baml == settled
    });
    if let judgement: Judgement = recorded.at(0) {
        assert.equal(judgement.verdict, "divergent");
        assert.is_true((judgement.divergence ?? "").includes("returns -1"));
        assert.is_true(judgement.verified);
    } else {
        baml.sys.panic("the ruling was not applied");
    }
}

test "name matches are put to the verifier like anything else" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let requests = verify_requests(matrix);
    assert.is_true(requests.length() > 0);
    let all = requests
        .map((request) -> {
            request.pair_lines.join("\n")
        })
        .join("\n");
    assert.is_true(all.includes("names and owners agree"));
}

test "the sweep can overturn an absence but not a pairing" {
    let matrix = fixture_matrix("sha-x", "5.8.3-test");
    let absent: string[] = [];
    matrix
        .judgements
        .push(
            Judgement {
                baml: "M:baml.Foo.from_parts",
                ts: null,
                verdict: "none",
                basis: "model",
                confidence: "low",
                reason: "the pool held nothing",
                divergence: null,
                rejected: absent,
                verified: false,
            },
        );
    // The sweep answers about TypeScript symbols: `baml` holds the TS id.
    let outcome = apply_sweep(
        matrix,
        fixture_proposals([fixture_proposal("Foo.prototype.only", "match", "M:baml.Foo.from_parts")]),
    );
    assert.equal(outcome.swept, 1);
    let recorded = outcome.matrix.judgements.filter((judgement) -> {
        judgement.baml == "M:baml.Foo.from_parts"
    });
    // One judgement, not two: the absence it overturned is gone.
    assert.equal(recorded.length(), 1);
    if let judgement: Judgement = recorded.at(0) {
        assert.equal(judgement.verdict, "match");
        assert.equal(judgement.ts ?? "", "Foo.prototype.only");
    } else {
        baml.sys.panic("the sweep did not land");
    }
}

test "reviewing everything carries nothing but keeps the previous conclusions" {
    // A revision that concluded something about a symbol the fresh run has not
    // judged — which is what every model judgement looks like on the next run.
    let source = fixture_matrix("sha-a", "5.8.3-test");
    let absent: string[] = [];
    source
        .judgements
        .push(
            Judgement {
                baml: "M:baml.Foo.from_parts",
                ts: null,
                verdict: "none",
                basis: "model",
                confidence: "high",
                reason: "TypeScript builds these inline",
                divergence: null,
                rejected: absent,
                verified: true,
            },
        );
    let before = fixture_revision(source);

    // Carrying: nothing has changed, so the conclusion is kept without asking.
    let carried = apply_revision(fixture_matrix("sha-a", "5.8.3-test"), before);
    assert.is_true(carried.carried > 0);
    let kept = carried.matrix.judgements.filter((judgement) -> {
        judgement.baml == "M:baml.Foo.from_parts"
    });
    assert.equal(kept.length(), 1);

    // Reviewing: the same revision is still in hand, and its conclusions are
    // put in front of the model so it can bless them — but nothing is taken on
    // trust, and the symbol is asked about again.
    let matrix = fixture_matrix("sha-a", "5.8.3-test");
    let requests = propose_requests(matrix, build_pools(matrix, pending_symbols(matrix)), before);
    let shown = requests
        .map((request) -> {
            request.prior_lines.join("\n")
        })
        .join("\n");
    assert.is_true(shown.includes("M:baml.Foo.from_parts"));
    assert.is_true(shown.includes("was none with nothing"));
    assert.is_true(shown.includes("TypeScript builds these inline"));
}

test "the concurrency cap falls back when nothing was asked for" {
    assert.equal(effective_limit(0), call_limit());
    assert.equal(effective_limit(-1), call_limit());
    assert.equal(effective_limit(24), 24);
}
