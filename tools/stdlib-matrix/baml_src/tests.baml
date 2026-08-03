// Unit tests over synthetic fixtures. Export-side fixtures are JSON strings
// (not struct literals) for two reasons: they must go through the real decode
// path to exercise the `baml.FromJson` override on ExportSignature, and
// ExportImpl cannot be built as a literal (its `interface` field name is
// rejected by the parser in literal position).

function fixture_export_text() -> string {
    #"{
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
            "throws": {
              "declared": {"display": "never"},
              "effective": {"display": "baml.panics.IndexOutOfBounds"},
              "panics": [{"display": "baml.panics.IndexOutOfBounds"}],
              "errors": []
            }
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
            "throws": {
              "effective": {"display": "CompareError"},
              "panics": [],
              "errors": [{"display": "CompareError"}]
            }
          }
        },
        {"name": "index_of", "signature": {"params": [{"name": "self", "ty": {"display": "baml.Foo"}}]}}
      ]
    }
  ]
}"#
}

function fixture_ts_text() -> string {
    #"{
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
}"#
}

function fixture_row() -> ContainerPair {
    ContainerPair {
        key: "foo",
        baml_types: ["baml.Foo"],
        baml_namespaces: [],
        ts_containers: ["Foo"],
        synonyms: [NamePair { baml_name: "sort_helper", ts_name: "sortStuff" }],
        note: null,
    }
}

test "match keys meet across snake_case and camelCase" {
    assert.equal(match_key("to_upper_case"), "touppercase");
    assert.equal(match_key("toUpperCase"), "touppercase");
    assert.equal(match_key("index_of"), match_key("indexOf"));
}

test "id helpers split kind prefix, container, and segments" {
    assert.equal(container_of("T:baml.time.Duration"), "baml.time");
    assert.equal(container_of("V:baml.helper_fn"), "baml");
    assert.equal(strip_kind_prefix("X:baml.impl[baml.Comparable for baml.Foo]"), "baml.impl[baml.Comparable for baml.Foo]");
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
                if let spec: ExportThrows = sig.throws_spec {
                    assert.equal(spec.panics.length(), 1);
                    assert.equal(spec.panics.at(0)?.display ?? "", "baml.panics.IndexOutOfBounds");
                } else {
                    baml.sys.panic("throws_spec missing after rename");
                }
            } else {
                baml.sys.panic("index_of signature missing");
            }
        }
    } else {
        baml.sys.panic("fixture items empty");
    }
}

test "container report matches, applies synonyms, and keeps impl provenance" {
    let export_doc = parse_export_text(fixture_export_text());
    let ts_doc = baml.json.from_string<TsSurfaceDoc>(fixture_ts_text());
    let row = fixture_row();
    let index = index_export(export_doc);
    let report = build_container_report(
        row,
        pool_baml_side(index, row),
        pool_ts_side(ts_doc, row),
    );

    // index_of ↔ indexOf, sort_helper ↔ sortStuff (synonym)
    assert.equal(report.matched, 2);
    // _size field, from_parts, compare (impl-contributed)
    assert.equal(report.baml_only, 3);
    // only
    assert.equal(report.ts_only, 1);

    for (let member_row in report.rows) {
        if (member_row.key == "compare") {
            if let member: PooledMember = member_row.baml {
                assert.equal(member.via ?? "", "baml.Comparable");
                assert.equal(member.errors.length(), 1);
            } else {
                baml.sys.panic("compare row lost its BAML member");
            }
        }
        if (member_row.key == "indexof") {
            assert.equal(member_row.status, "matched");
            // The impl's competing index_of is shadowed by the class method,
            // not pooled as an alternate.
            assert.equal(member_row.baml_alternates.length(), 0);
            if let member: PooledMember = member_row.baml {
                assert.equal(member.panics.at(0) ?? "", "baml.panics.IndexOutOfBounds");
            }
        }
        if (member_row.key == "sortstuff") {
            assert.equal(member_row.status, "matched");
        }
    }
}

test "namespace pooling hides synthetics and qualifies displays" {
    let export_doc = parse_export_text(fixture_export_text());
    let index = index_export(export_doc);
    let row = ContainerPair {
        key: "root",
        baml_types: [],
        baml_namespaces: ["baml"],
        ts_containers: [],
        synonyms: [],
        note: null,
    };
    let acc = pool_baml_side(index, row);
    assert.equal(acc.synthetic_hidden, 1);
    let displays = acc.members.map(
        (member) -> {
            member.display
        },
    );
    assert.is_true(displays.includes("baml.helper_fn"));
    assert.is_true(displays.includes("Foo.index_of"));
}

test "mapping gap detection flags unscoped and stale surfaces" {
    let export_doc = parse_export_text(fixture_export_text());
    let ts_doc = baml.json.from_string<TsSurfaceDoc>(fixture_ts_text());
    let stale: UncomparedEntry[] = [UncomparedEntry { name: "baml.nonexistent", reason: "gone" }];
    let gaps = mapping_gaps(export_doc, ts_doc, [fixture_row()], stale, []);
    // helper_fn leaves the root namespace unscoped; the stale entry is flagged.
    assert.equal(gaps.length(), 2);
    assert.is_true((gaps.at(0) ?? "").includes("baml"));
    assert.is_true((gaps.at(1) ?? "").includes("baml.nonexistent"));
}
