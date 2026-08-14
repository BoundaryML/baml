function fixture_config(repo_root: string, source_directory: string) -> SourceConfig {
    let base = source_config_for(source_configs(repo_root), source_directory);
    let absolute_fixture_root = `${repo_root}/tools/sdk-parity-lint/test-fixtures/${base.language}`;
    let fixture_root = absolute_fixture_root.substring(repo_root.length() + 1, absolute_fixture_root.length());
    make_source_config(
        base.language,
        base.source_directory,
        fixture_root,
        base.rule_file,
        base.file_glob,
        base.suffix,
        base.declaration_rule_ids,
        base.raw_name_field,
        base.prefix,
    )
}

function declaration_count(scan: SourceScan) -> int {
    let count = 0;
    for (let ast_match in scan.matches) {
        if (array_contains(scan.config.declaration_rule_ids, ast_match.ruleId)) {
            count += 1;
        }
    }
    count
}

function fixture_ast_match(rule_id: string, start: int, end: int) -> AstMatch {
    AstMatch {
        text: "fixture test declaration",
        file: "tools/sdk-parity-lint/test-fixtures/typescript/fixture/customizable/sample.test.ts",
        range: AstRange {
            byteOffset: AstByteOffset { start: start, end: end },
            start: AstPosition { line: 0, column: start },
            end: AstPosition { line: 0, column: end },
        },
        metaVariables: null,
        ruleId: rule_id,
    }
}

test "ast-grep rules find framework tests and ignore ordinary helpers" {
    let repo_root = ".";
    let python = scan_source(repo_root, fixture_config(repo_root, "python_pydantic2"));
    let typescript = scan_source(repo_root, fixture_config(repo_root, "typescript"));
    let cpp = scan_source(repo_root, fixture_config(repo_root, "cpp"));
    let csharp = scan_source(repo_root, fixture_config(repo_root, "csharp"));
    let rust = scan_source(repo_root, fixture_config(repo_root, "rust"));
    let go = scan_source(repo_root, fixture_config(repo_root, "go"));
    let java = scan_source(repo_root, fixture_config(repo_root, "java"));
    let swift = scan_source(repo_root, fixture_config(repo_root, "swift"));
    validate_audit_coverage(python);
    validate_audit_coverage(typescript);
    validate_audit_coverage(cpp);
    validate_audit_coverage(csharp);
    validate_audit_coverage(rust);
    validate_audit_coverage(go);
    validate_audit_coverage(java);
    validate_audit_coverage(swift);
    assert.equal(declaration_count(python), 2);
    assert.equal(declaration_count(typescript), 8);
    assert.equal(declaration_count(cpp), 1);
    assert.equal(declaration_count(csharp), 2);
    assert.equal(declaration_count(rust), 3);
    assert.equal(declaration_count(go), 1);
    assert.equal(declaration_count(java), 2);
    assert.equal(declaration_count(swift), 1)
}

test "audit coverage rejects unsupported TypeScript declaration forms" {
    let repo_root = ".";
    let config = fixture_config(repo_root, "typescript");
    let scan = SourceScan { config: config, matches: [fixture_ast_match("typescript-audit-modifier", 0, 24)] };
    {
        validate_audit_coverage(scan);
        baml.sys.panic("expected unsupported TypeScript declaration rejection")
    } catch (e) {
        baml.errors.ParseError => {
            assert.contains(e.message, "SDK test names must use the parity lint's canonical form");
            assert.contains(e.message, "SDK_PARITY_LINT(skip): <reason>");
            null
        }
    }
}

test "audit coverage rejects C# wrappers without canonical test IDs" {
    let repo_root = ".";
    let config = fixture_config(repo_root, "csharp");
    let scan = SourceScan { config: config, matches: [fixture_ast_match("csharp-audit-test", 0, 24)] };
    {
        validate_audit_coverage(scan);
        baml.sys.panic("expected noncanonical C# test declaration rejection")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "audit coverage keeps the widest TypeScript audit match" {
    let repo_root = ".";
    let config = fixture_config(repo_root, "typescript");
    let scan = SourceScan {
        config: config,
        matches: [
            fixture_ast_match("typescript-runtime-test", 0, 40),
            fixture_ast_match("typescript-audit-modifier", 0, 20),
            fixture_ast_match("typescript-audit-curried", 0, 40),
        ],
    };
    validate_audit_coverage(scan)
}

test "normalization derives category IDs and associates an annotation" {
    let repo_root = ".";
    let scan = scan_source(repo_root, fixture_config(repo_root, "python_pydantic2"));
    let tests = discover_tests([scan]);
    let annotations = build_annotation_configs(repo_root, [scan], tests);
    let ids = tests
        .map(
            (test_case) -> {
                test_case.sdk_test_id
            },
        );
    ids.sort_by_key(
        (sdk_test_id) -> {
            sdk_test_id
        },
    );
    assert.equal(tests.length(), 2);
    assert.equal(ids, ["fixture/async_case", "fixture/sync_case"]);
    assert.equal(annotations.length(), 1);
    assert.equal(annotations[0].sdk_test_id, "fixture/sync_case");
    assert.equal(annotations[0].reason, "fixture-only behavior")
}

test "dynamic TypeScript titles are rejected instead of undercounted" {
    {
        let _ = decode_typescript_id("`${prefix}_dynamic_case`", "fixture/customizable/sample.test.ts", 1);
        baml.sys.panic("expected dynamic title rejection")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "double-quoted TypeScript titles are decoded before validation" {
    assert
        .equal(
            decode_typescript_id("\"escaped\\u005fcase\"", "fixture/customizable/sample.test.ts", 1),
            "escaped_case",
        )
}

test "single-quoted TypeScript titles use JavaScript escape semantics" {
    assert
        .equal(
            decode_typescript_id("'escaped\\x5fcase'", "fixture/customizable/sample.test.ts", 1),
            "escaped_case",
        );
    assert
        .equal(
            decode_typescript_id("'escaped\\u{5f}case'", "fixture/customizable/sample.test.ts", 1),
            "escaped_case",
        )
}

test "TypeScript runtime conditions are explicit and intersect deterministically" {
    assert.equal(runtimes_for_condition("isTestRuntime(\"node\")", "x.ts", 1), ["node"]);
    assert.equal(runtimes_for_condition("isWebRuntime", "x.ts", 1), ["web", "workers"]);
    assert
        .equal(
            intersect_runtimes(["node", "web", "workers"], ["web", "workers"]),
            ["web", "workers"],
        )
}

test "TypeScript direct and enclosing suite gates set runtime membership" {
    let repo_root = ".";
    let scan = scan_source(repo_root, fixture_config(repo_root, "typescript"));
    let valid_matches: AstMatch[] = [];
    for (let ast_match in scan.matches) {
        if (!ast_match.text.matches("dynamic_case")) {
            valid_matches.push(ast_match);
        }
    }
    let valid_scan = SourceScan { config: scan.config, matches: valid_matches };
    let tests = discover_tests([valid_scan]);
    apply_typescript_runtime_gates(tests, [valid_scan]);
    let direct = match (tests.find((test_case: DiscoveredTest) -> bool {
        test_case.sdk_test_id == "fixture/runtime_case"
    })) {
        let found: DiscoveredTest => found,
        null => baml.sys.panic("expected direct runtime-gated fixture"),
    };
    let suite = match (tests.find((test_case: DiscoveredTest) -> bool {
        test_case.sdk_test_id == "fixture/suite_runtime_case"
    })) {
        let found: DiscoveredTest => found,
        null => baml.sys.panic("expected suite runtime-gated fixture"),
    };
    assert.equal(direct.runtimes, ["web"]);
    assert.equal(suite.runtimes, ["web", "workers"])
}

test "skip annotation waives absent environments but ratchets present declarations" {
    let sdk_envs = ["python_pydantic2", "rust"];
    let inventories = [
        SdkInventory {
            sdk_env: "python_pydantic2",
            source_root: "python",
            tests: [
                InventoryTest {
                    sdk_test_id: "fixture/shared",
                    raw_name: "test_shared",
                    source_file: "a.py",
                    source_line: 1,
                    source_column: 1,
                    rule_id: "python-test",
                },
            ],
        },
        SdkInventory { sdk_env: "rust", source_root: "rust", tests: [] },
    ];
    let annotations = [
        AnnotationConfig {
            sdk_test_id: "fixture/shared",
            reason: "Python-only fixture",
            source_file: "a.py",
            source_line: 1,
        },
    ];
    let report = build_parity_report(sdk_envs, inventories, annotations);
    assert.equal(report.required_gap_count, 0);
    let markdown = render_parity_report(report, "ast-grep test");
    assert.contains(markdown, "| fixture/shared | y | - | python_pydantic2 | Python-only fixture |")
}

test "unannotated parity absence is a required gap" {
    let sdk_envs = ["python_pydantic2", "rust"];
    let inventories = [
        SdkInventory {
            sdk_env: "python_pydantic2",
            source_root: "python",
            tests: [
                InventoryTest {
                    sdk_test_id: "fixture/shared",
                    raw_name: "test_shared",
                    source_file: "a.py",
                    source_line: 1,
                    source_column: 1,
                    rule_id: "python-test",
                },
            ],
        },
        SdkInventory { sdk_env: "rust", source_root: "rust", tests: [] },
    ];
    let report = build_parity_report(sdk_envs, inventories, []);
    assert.equal(report.required_gap_count, 1);
    assert.equal(report.rows[0].has_required_gap, true)
}

test "Python-baselined parity ignores SDK-only test IDs" {
    let sdk_envs = ["python_pydantic2", "rust", "go"];
    let report = fixture_parity_report(
        sdk_envs,
        [
            fixture_parity_row("type_shapes/shared", sdk_envs, [true, true, false], sdk_envs, ""),
            fixture_parity_row("type_shapes/python_only", sdk_envs, [true, false, false], sdk_envs, ""),
            fixture_parity_row("type_shapes/rust_only", sdk_envs, [false, true, false], sdk_envs, ""),
        ],
    );
    let rendered = render_python_baseline_parity(report).join("\n");
    assert.contains(rendered, "the 2 test IDs declared in `python_pydantic2`");
    assert.contains(rendered, "| python_pydantic2 | 2 / 2 | 100.0% |");
    assert.contains(rendered, "| rust | 1 / 2 | 50.0% |");
    assert.contains(rendered, "| go | 0 / 2 | 0.0% |");
    assert.equal(render_percentage(1, 3), "33.3%");
    assert.equal(render_percentage(2, 3), "66.7%");
    assert.equal(render_percentage(0, 0), "n/a")
}

function fixture_parity_row(
    sdk_test_id: string,
    sdk_envs: string[],
    coverage: bool[],
    required_envs: string[],
    reason: string,
) -> ParityRow {
    let cells: ParityCell[] = [];
    let has_required_gap = false;
    let index = 0;
    while (index < sdk_envs.length()) {
        let required = array_contains(required_envs, sdk_envs[index]);
        if (required && !coverage[index]) {
            has_required_gap = true;
        }
        cells
            .push(
                ParityCell {
                    sdk_env: sdk_envs[index],
                    present: coverage[index],
                    required: required,
                },
            );
        index += 1;
    }
    ParityRow {
        sdk_test_id: sdk_test_id,
        cells: cells,
        required_envs: required_envs,
        reason: reason,
        has_required_gap: has_required_gap,
    }
}

function fixture_parity_report(sdk_envs: string[], rows: ParityRow[]) -> ParityReport {
    let missing = 0;
    for (let row in rows) {
        for (let cell in row.cells) {
            if (cell.required && !cell.present) {
                missing += 1;
            }
        }
    }
    ParityReport { sdk_envs: sdk_envs, rows: rows, required_gap_count: missing }
}

test "baseline report round trips escaped Markdown cells" {
    let sdk_envs = sdk_environments();
    let row = fixture_parity_row(
        "type_shapes/round_trip",
        sdk_envs,
        [true, false, true, false, true, false, true, false, true, false],
        ["python_pydantic2", "typescript_node"],
        "reason with a | pipe and \\ slash",
    );
    let report = fixture_parity_report(sdk_envs, [row]);
    let comparison = compare_to_baseline(report, report);
    let parsed = parse_baseline(render_ratchet_report(report, comparison), "<fixture>", sdk_envs);
    assert.equal(parsed.rows.length(), 1);
    assert.equal(parsed.rows[0].sdk_test_id, row.sdk_test_id);
    assert.equal(parsed.rows[0].required_envs, row.required_envs);
    assert.equal(parsed.rows[0].reason, row.reason)
}

test "baseline ratchet passes improvements without regressions" {
    let sdk_envs = ["python_pydantic2", "typescript_node"];
    let required = sdk_envs;
    let baseline = fixture_parity_report(sdk_envs, [fixture_parity_row("type_shapes/case", sdk_envs, [true, false], required, "")]);
    let current = fixture_parity_report(sdk_envs, [fixture_parity_row("type_shapes/case", sdk_envs, [true, true], required, "")]);
    let comparison = compare_to_baseline(baseline, current);
    assert.equal(comparison.outcome, "better");
    assert.equal(comparison.regressions, []);
    assert
        .equal(
            comparison.resolved,
            [ParityPair { sdk_test_id: "type_shapes/case", sdk_env: "typescript_node" }],
        )
}

test "baseline ratchet treats deleting a declaration as worse" {
    let sdk_envs = ["python_pydantic2", "typescript_node"];
    let baseline = fixture_parity_report(sdk_envs, [fixture_parity_row("type_shapes/deleted", sdk_envs, [true, false], sdk_envs, "")]);
    let current = fixture_parity_report(sdk_envs, []);
    let comparison = compare_to_baseline(baseline, current);
    assert.equal(comparison.outcome, "worse");
    assert.equal(comparison.baseline_present_count, 1);
    assert.equal(comparison.current_present_count, 0)
}

test "baseline ratchet rejects weakened annotated requirements" {
    let sdk_envs = ["python_pydantic2", "typescript_node"];
    let baseline = fixture_parity_report(
        sdk_envs,
        [
            fixture_parity_row("type_shapes/annotated", sdk_envs, [true, false], sdk_envs, "old annotation"),
        ],
    );
    let current = fixture_parity_report(
        sdk_envs,
        [
            fixture_parity_row(
                "type_shapes/annotated",
                sdk_envs,
                [true, false],
                ["python_pydantic2"],
                "weakened annotation",
            ),
        ],
    );
    let comparison = compare_to_baseline(baseline, current);
    assert.equal(comparison.outcome, "worse");
    assert
        .equal(
            comparison.weakened_requirements,
            [ParityPair { sdk_test_id: "type_shapes/annotated", sdk_env: "typescript_node" }],
        )
}

test "TypeScript runtime membership narrows only TypeScript requirements" {
    let sdk_envs = sdk_environments();
    let test_case = DiscoveredTest {
        language: "typescript",
        source_directory: "typescript",
        sdk_test_id: "type_shapes/web_case",
        raw_name: "web_case",
        source_file: "runtime.test.ts",
        source_line: 1,
        source_column: 1,
        match_start: 0,
        match_end: 1,
        match_start_line: 1,
        rule_id: "typescript-runtime-test",
        runtime_condition: "isWebRuntime",
        runtimes: ["web", "workers"],
    };
    let required = typescript_required_environments("type_shapes/web_case", sdk_envs, [test_case]);
    assert.equal(array_contains(required, "typescript_node"), false);
    assert.equal(array_contains(required, "typescript_web_chromium"), true);
    assert.equal(array_contains(required, "typescript_web_cloudflare_workers"), true);
    assert.equal(array_contains(required, "python_pydantic2"), true)
}
