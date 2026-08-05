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
                assert.equal(member.errors ?? "", "CompareError");
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
                assert.equal(member.errors ?? "", "baml.panics.IndexOutOfBounds");
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

// ── ratchet ─────────────────────────────────────────────────────────────────
// Baselines are produced by the real renderer rather than hand-written, so
// these also guard the summary table's parse/render round trip.

// BUG(baml compiler): a struct literal may omit a required, non-optional
// field. It compiles clean and the field is `null` at runtime, so a value
// typed `int` holds `null`. Minimal repro (verified 2026-08-05):
//
//   class Two { a: int, b: int }
//   let t = Two { a: 1 };     // no error
//   `${t.b}`                  // → "null"
//
// It surfaced here rather than at the literal: adding `aligned: int` to the
// report model left this fixture silently short a field, and the only symptom
// was `JsonDecodeError { message: "expected integer" }` from the round-trip
// test — decoding caught what construction did not. Keep the fixtures in step
// with the models by hand until literals are checked for completeness.
function fixture_report_with_sha(
    key: string,
    matched: int,
    baml_only: int,
    ts_only: int,
    sha: string,
) -> MatrixReport {
    MatrixReport {
        format_version: 1,
        provenance: MatrixProvenance {
            baml_surface_sha256: sha,
            export_format_version: 1,
            typescript_version: "5.8.3-test",
            types_node_version: null,
        },
        uncompared_baml: [],
        uncompared_ts: [],
        containers: [
            ContainerReport {
                key: key,
                note: null,
                baml_origins: ["baml.X"],
                ts_origins: ["X"],
                matched: matched,
                aligned: 0,
                baml_only: baml_only,
                ts_only: ts_only,
                synthetic_hidden: 0,
                rows: [],
            },
        ],
        totals: MatrixTotals {
            containers: 1,
            matched: matched,
            aligned: 0,
            baml_only: baml_only,
            ts_only: ts_only,
            synthetic_hidden: 0,
        },
    }
}

/// Same inputs every time, so a difference in counts alone leaves the report
/// up to date — the ratchet keys on provenance, not on the body.
function fixture_report(key: string, matched: int, baml_only: int, ts_only: int) -> MatrixReport {
    fixture_report_with_sha(key, matched, baml_only, ts_only, "sha-unchanged")
}

test "an unchanged run is up to date and reports no deltas" {
    let report = fixture_report("string", 10, 5, 7);
    let comparison = compare_to_baseline(report, report);
    assert.is_true(comparison.up_to_date);
    assert.is_true(comparison.baseline_present);
    assert.is_true(!comparison.body_differs);
    assert.equal(comparison.deltas.length(), 0);
    assert.equal(comparison.regressions.length(), 0);
}

test "a changed BAML surface makes the report stale and names the input" {
    let baseline = fixture_report_with_sha("string", 10, 5, 7, "sha-old");
    let now = fixture_report_with_sha("string", 9, 5, 7, "sha-new");
    let comparison = compare_to_baseline(baseline, now);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.input_changes.length(), 1);
    assert.is_true((comparison.input_changes.at(0) ?? "").includes("BAML stdlib surface changed"));
    assert.equal(comparison.regressions.length(), 1);
    assert.is_true((comparison.regressions.at(0) ?? "").includes("shrank"));
}

test "a TypeScript version bump is an input change on its own" {
    let baseline = fixture_report("string", 10, 5, 7);
    let now = fixture_report("string", 10, 5, 7);
    now.provenance.typescript_version = "9.9.9-next";
    let comparison = compare_to_baseline(baseline, now);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.input_changes.length(), 1);
    assert.is_true((comparison.input_changes.at(0) ?? "").includes("TypeScript"));
}

test "a structure-only difference leaves the report up to date but is flagged" {
    // Same inputs, different counts: only the tool can have changed, which a
    // developer fixes by regenerating rather than something CI should fail on.
    let baseline = fixture_report("string", 10, 5, 7);
    let now = fixture_report("string", 12, 3, 5);
    let comparison = compare_to_baseline(baseline, now);
    assert.is_true(comparison.up_to_date);
    assert.is_true(comparison.body_differs);
    assert.equal(comparison.input_changes.length(), 0);
    assert.equal(comparison.deltas.length(), 0);
}

test "dropping a container is a regression and adding one is an improvement" {
    let baseline = fixture_report_with_sha("string", 10, 5, 7, "sha-old");
    let now = fixture_report_with_sha("bigint", 1, 1, 1, "sha-new");
    let comparison = compare_to_baseline(baseline, now);
    assert.equal(comparison.regressions.length(), 1);
    assert.is_true((comparison.regressions.at(0) ?? "").includes("no longer compared"));
    assert.equal(comparison.improvements.length(), 1);
    assert.is_true((comparison.improvements.at(0) ?? "").includes("newly compared"));
}

test "the canonical report round-trips through JSON" {
    let report = fixture_report_with_sha("string", 10, 5, 7, "abc123def456789");
    let decoded = baml.json.to<MatrixReport>(baml.json.parse(baml.json.serialize(report)));
    assert.equal(decoded.provenance.baml_surface_sha256, "abc123def456789");
    assert.equal(decoded.provenance.typescript_version, "5.8.3-test");
    assert.is_true(decoded.provenance.types_node_version == null);
    assert.equal(decoded.containers.length(), 1);
    assert.equal(decoded.totals.matched, 10);
    // A round-tripped report must compare equal to its source, or the ratchet
    // would report drift on every run.
    assert.is_true(compare_to_baseline(decoded, report).up_to_date);
    assert.is_true(!compare_to_baseline(decoded, report).body_differs);
}

test "a missing baseline is reported, not treated as a regression" {
    let report = fixture_report("string", 10, 5, 7);
    let comparison = compare_to_baseline(null, report);
    assert.is_true(!comparison.baseline_present);
    assert.is_true(!comparison.up_to_date);
    assert.equal(comparison.regressions.length(), 0);
    assert.equal(comparison.deltas.length(), 0);
}
