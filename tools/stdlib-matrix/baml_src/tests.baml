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
              "fields": [{"name": "_size", "ty": {"display": "int"}}],
              "methods": [
                {
                  "name": "index_of",
                  "signature": {
                    "params": [{"name": "self", "ty": {"display": "baml.Foo"}}],
                    "returns": {"display": "int"},
                    "throws": {"display": "baml.panics.IndexOutOfBounds"}
                  }
                },
                {
                  "name": "from_parts",
                  "signature": {"params": [{"name": "parts", "ty": {"display": "int"}}], "returns": {"display": "baml.Foo"}}
                },
                {
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
                  "name": "compare",
                  "signature": {
                    "params": [{"name": "self", "ty": {"display": "baml.Foo"}}],
                    "returns": {"display": "int"},
                    "throws": {"display": "CompareError"}
                  }
                },
                {"name": "index_of", "signature": {"params": [{"name": "self", "ty": {"display": "baml.Foo"}}]}}
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
    // resolve back to real symbols through their indices.
    assert.is_true(matrix.correspondences.length() >= 1);
    if let first: Correspondence = matrix.correspondences.at(0) {
        assert.equal(first.basis, "name");
        if let baml_side: BamlSymbol = matrix.baml.at(first.baml) {
            if let ts_side: TsSymbol = matrix.ts.at(first.ts) {
                assert.equal(match_key(baml_name_of(baml_side)), match_key(ts_name_of(ts_side)));
            } else {
                baml.sys.panic("correspondence points past the TS list");
            }
        } else {
            baml.sys.panic("correspondence points past the BAML list");
        }
    }
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
    assert.is_true(!comparison.body_differs);
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
    assert.equal(decoded.counts.correspondences, matrix.counts.correspondences);
    // A round-tripped header must compare equal to its source, or the ratchet
    // would report drift on every run.
    assert.is_true(compare_to_baseline(decoded, matrix).up_to_date);
    assert.is_true(!compare_to_baseline(decoded, matrix).body_differs);
}

test "the markdown view renders both sides" {
    let markdown = render_matrix_markdown(fixture_matrix("sha-a", "5.8.3-test"));
    assert.is_true(markdown.includes("# BAML ↔ TypeScript stdlib matrix"));
    assert.is_true(markdown.includes("## BAML surface"));
    assert.is_true(markdown.includes("TypeScript symbols with no BAML counterpart"));
    // An impl method groups under the impl it arrives through.
    assert.is_true(markdown.includes("baml.Foo as baml.Comparable"));
}
