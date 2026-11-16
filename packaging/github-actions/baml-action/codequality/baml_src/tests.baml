function fn_report(
    name: string,
    file: string = "baml_src/main.baml",
    start_line: int = 1,
    end_line: int = 3,
    cognitive: bigint = 0n,
    cyclomatic: bigint = 1n,
    npath: bigint = 1n,
    nesting: bigint = 0n,
    exits: bigint = 1n,
) -> FunctionReport {
    FunctionReport {
        name: name,
        file: file,
        start_line: start_line,
        end_line: end_line,
        cognitive: cognitive,
        cyclomatic: cyclomatic,
        npath: npath,
        nesting: nesting,
        exits: exits,
    }
}

function report_of(functions: FunctionReport[]) -> Report {
    Report { functions: functions, files: [] }
}

function test_context() -> Context {
    Context {
        server_url: "https://github.com",
        repo: "acme/app",
        head_sha: "a41c9e2f00000000000000000000000000000000",
        base_label: "canary",
        project_prefix: "",
        project: ".",
    }
}

function names(changes: FunctionChange[]) -> string[] {
    changes.map((c) -> {
        c.report.name
    })
}

testset "compare" {
    test "unchanged reports nothing" {
        let r = report_of([fn_report("a", cognitive = 3n)]);
        let comparison = Comparison.new(r, r);
        assert.equal(comparison.worse, []);
        assert.equal(comparison.better, []);
        assert.equal(comparison.added, []);
        assert.equal(comparison.removed, 0);
        assert.is_true(!comparison.changed())
    }

    test "one row per function with every changed metric" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 2n, nesting = 1n, exits = 3n)]),
            report_of([fn_report("a", cognitive = 5n, nesting = 2n, exits = 1n)]),
        );
        assert.equal(comparison.worse.length(), 1);
        assert.equal(comparison.better, []);
        assert.equal(
            comparison.worse[0].changes,
            [
                MetricChange { metric: Metric.Cognitive, before: 2n, after: 5n },
                MetricChange { metric: Metric.Nesting, before: 1n, after: 2n },
                MetricChange { metric: Metric.Exits, before: 3n, after: 1n },
            ],
        )
    }

    test "only improvements is better" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 5n)]),
            report_of([fn_report("a", cognitive = 1n)]),
        );
        assert.equal(comparison.worse, []);
        assert.equal(names(comparison.better), ["a"])
    }

    test "worse sorted by relative increase" {
        let comparison = Comparison.new(
            report_of([fn_report("small", cognitive = 10n), fn_report("big", cognitive = 1n)]),
            report_of([fn_report("small", cognitive = 12n), fn_report("big", cognitive = 4n)]),
        );
        assert.equal(names(comparison.worse), ["big", "small"])
    }

    test "worse sorted by the largest metric increase" {
        let comparison = Comparison.new(
            report_of(
                [
                    fn_report("steady", cognitive = 10n, nesting = 1n),
                    fn_report("spiky", cognitive = 10n, nesting = 1n),
                ],
            ),
            report_of(
                [
                    fn_report("steady", cognitive = 15n, nesting = 1n),
                    fn_report("spiky", cognitive = 11n, nesting = 4n),
                ],
            ),
        );
        assert.equal(names(comparison.worse), ["spiky", "steady"])
    }

    test "increase from zero is worse" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 0n)]),
            report_of([fn_report("a", cognitive = 1n)]),
        );
        assert.equal(
            comparison.worse[0].changes,
            [MetricChange { metric: Metric.Cognitive, before: 0n, after: 1n }],
        )
    }

    test "same name in different files are different functions" {
        let comparison = Comparison.new(
            report_of([fn_report("a", file = "x.baml", cognitive = 1n)]),
            report_of(
                [
                    fn_report("a", file = "x.baml", cognitive = 1n),
                    fn_report("a", file = "y.baml", cognitive = 9n),
                ],
            ),
        );
        assert.equal(
            comparison.added.map((f) -> {
                f.file
            }),
            ["y.baml"],
        );
        assert.equal(comparison.worse, [])
    }

    test "added and removed" {
        let comparison = Comparison.new(
            report_of([fn_report("gone")]),
            report_of([fn_report("fresh", cognitive = 4n)]),
        );
        assert.equal(
            comparison.added.map((f) -> {
                f.name
            }),
            ["fresh"],
        );
        assert.equal(comparison.removed, 1);
        assert.is_true(comparison.changed())
    }

    test "npath beyond float range" {
        let huge = 10n.pow(400n);
        let comparison = Comparison.new(
            report_of([fn_report("a", npath = huge), fn_report("b", npath = 2n)]),
            report_of([fn_report("a", npath = huge * 3n), fn_report("b", npath = 4n)]),
        );
        assert.equal(names(comparison.worse), ["a", "b"])
    }
}

testset "parse_report" {
    test "decodes npath exactly and ignores other fields" {
        let report = Report.parse(
            `{"functions": [{"name": "a", "file": "baml_src\\\\main.baml", "start_line": 1, "end_line": 3, "lines": 3, "params": 0, "cognitive": 0, "cyclomatic": 1, "npath": 123456789012345678901234567890, "nesting": 0, "exits": 1, "fan_in": 0, "fan_out": 0}], "files": []}`,
        );
        assert.equal(report.functions[0].npath, 123456789012345678901234567890n);
        assert.equal(report.functions[0].file, "baml_src/main.baml")
    }
}

testset "render" {
    test "significant functions are one row each, worse first" {
        let comparison = Comparison.new(
            report_of(
                [
                    fn_report(
                        "Router.dispatch",
                        start_line = 10,
                        end_line = 40,
                        cognitive = 9n,
                        nesting = 2n,
                    ),
                    fn_report("Pipeline.run", cognitive = 22n),
                ],
            ),
            report_of(
                [
                    fn_report(
                        "Router.dispatch",
                        start_line = 10,
                        end_line = 42,
                        cognitive = 18n,
                        nesting = 1n,
                    ),
                    fn_report("Pipeline.run", cognitive = 8n),
                ],
            ),
        );
        let body = render(comparison, test_context());
        assert.is_true(
            body.starts_with(
                "<!-- baml-action:codequality:. -->\n**1 function got worse**, 1 got better.\n",
            ),
        );
        assert.contains(
            body,
            "| [`Router.dispatch`](https://github.com/acme/app/blob/a41c9e2f00000000000000000000000000000000/baml_src/main.baml#L10-L42) | **Worse**: cognitive 9 &rarr; 18, nesting 2 &rarr; 1 |\n| [`Pipeline.run`]",
        );
        assert.contains(body, "| **Better**: cognitive 22 &rarr; 8 |");
        assert.contains(body, "<summary>What these mean</summary>");
        assert.contains(body, "Compared `a41c9e2` with `canary` using `baml codequality`.")
    }

    test "headline" {
        let one = report_of([fn_report("a", cognitive = 5n)]);
        assert.equal(Comparison.new(one, one).headline(), "**No function changed complexity.**");
        assert.equal(
            Comparison.new(one, report_of([fn_report("a", cognitive = 1n)])).headline(),
            "**No function got worse**, 1 got better.",
        );
        assert.equal(
            Comparison
                .new(
                    Report {
                        functions: [fn_report("a"), fn_report("b")],
                        files: [file_report("x.baml", 35n)],
                    },
                    Report {
                        functions: [fn_report("a"), fn_report("c")],
                        files: [file_report("x.baml", 42n)],
                    },
                )
                .headline(),
            "**No function got worse**. 1 new, 1 removed. Lines 35 &rarr; 42.",
        )
    }

    test "small changes are left out" {
        let comparison = Comparison.new(
            report_of([fn_report("small", cognitive = 2n), fn_report("big", cognitive = 2n)]),
            report_of([fn_report("small", cognitive = 4n), fn_report("big", cognitive = 5n)]),
        );
        let body = render(comparison, test_context());
        assert.contains(body, "**2 functions got worse**.");
        assert.contains(body, "[`big`]");
        assert.is_true(!body.includes("[`small`]"))
    }

    test "no table when nothing changed significantly" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 2n)]),
            report_of([fn_report("a", cognitive = 3n)]),
        );
        assert.is_true(!render(comparison, test_context()).includes("| Function |"))
    }

    test "project prefix in links and marker" {
        let ctx = test_context();
        ctx.project_prefix = "services/agent/";
        ctx.project = "services/agent";
        let body = render(
            Comparison.new(report_of([fn_report("a")]), report_of([fn_report("a", cognitive = 5n)])),
            ctx,
        );
        assert.contains(
            body,
            "/blob/a41c9e2f00000000000000000000000000000000/services/agent/baml_src/main.baml#L1-L3",
        );
        assert.is_true(body.starts_with("<!-- baml-action:codequality:services/agent -->"))
    }

    test "unmeasured base" {
        let body = render(Unmeasured {}, test_context());
        assert.contains(body, "Could not measure `canary`");
        assert.is_true(!body.includes("What these mean"))
    }

    test "rows are capped" {
        let count = MAX_ROWS + 3;
        let base = report_of(
            baml.Array.generate(count, (i) -> {
                fn_report(`f${i}`, cognitive = 1n)
            }),
        );
        let head = report_of(
            baml.Array.generate(count, (i) -> {
                fn_report(`f${i}`, cognitive = 5n)
            }),
        );
        let body = render(Comparison.new(base, head), test_context());
        assert.contains(body, "...and 3 more.");
        assert.equal(body.split("\n| [`f").length() - 1, MAX_ROWS)
    }
}

testset "render_reports" {
    let head = `{"functions": [{"name": "a", "file": "baml_src/main.baml", "start_line": 1, "end_line": 3, "cognitive": 0, "cyclomatic": 1, "npath": 1, "nesting": 0, "exits": 1}], "files": []}`;

    test "unchanged" {
        let rendered = render_reports(head, head, test_context());
        assert.equal(rendered.changed, false);
        assert.contains(rendered.body, "**No function changed complexity.**")
    }

    test "missing base is unmeasured" {
        let rendered = render_reports(null, head, test_context());
        assert.is_true(rendered.changed);
        assert.contains(rendered.body, "Could not measure")
    }

    test "garbage base is unmeasured" {
        let rendered = render_reports("error: not json", head, test_context());
        assert.is_true(rendered.changed);
        assert.contains(rendered.body, "Could not measure")
    }

    test "normalize_prefix" {
        assert.equal(Context.normalize_prefix(""), "");
        assert.equal(Context.normalize_prefix("."), "");
        assert.equal(Context.normalize_prefix("services/agent/"), "services/agent/");
        assert.equal(Context.normalize_prefix("services\\agent\\"), "services/agent/")
    }
}

function file_report(file: string, lines: bigint) -> FileReport {
    FileReport { file: file, lines: lines }
}

testset "summary" {
    test "summarize" {
        let summary = (
            Report {
                functions: [
                    fn_report("a", cognitive = 3n, nesting = 1n, npath = 3n),
                    fn_report("b", cognitive = 12n, nesting = 3n, npath = 36n),
                ],
                files: [file_report("x.baml", 30n), file_report("y.baml", 12n)],
            }
        )
            .summary();
        assert.equal(summary.functions, 2);
        assert.equal(summary.lines, 42n);
        assert.equal(summary.average(Metric.Cognitive), Ratio { numerator: 15n, denominator: 2n });
        assert.equal(report_of([]).summary().average(Metric.NPath), null)
    }

    test "count reads as baml codequality writes it" {
        let max_u64 = 18446744073709551615n;
        assert.equal(
            [
                0n,
                999n,
                3170n,
                999999n,
                2700000n,
                273049159680n,
                9960000000000000000n,
                max_u64,
                max_u64 * 1000000000n,
            ]
                .map((n) -> {
                    count(n)
                }),
            ["0", "999", "3,170", "999,999", "2.7M", "273.0B", "1.0e19", "1.8e19", "1.8e28"],
        )
    }
}

testset "radar" {
    test "radar shows health relative to the base and capped" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 4n, npath = 20n, nesting = 1n)]),
            report_of([fn_report("a", cognitive = 8n, npath = 2n, nesting = 0n)]),
        );
        assert.equal(RadarAxis.of(comparison, Metric.Cognitive).health, 50n);
        assert.equal(RadarAxis.of(comparison, Metric.NPath).health, 1000n);
        assert.equal(RadarAxis.of(comparison, Metric.Nesting).health, null);
        assert.equal(RadarAxis.of(comparison, Metric.Exits).health, 100n);
        let body = render(comparison, test_context());
        assert.contains(
            body,
            `  axis cognitive["Readable 50%"], cyclomatic["Few branches 100%"], npath["Few paths 1,000%"], nesting["Flat >300%"], exits["Few exits 100%"]`,
        );
        assert.contains(body, `  curve before["canary"]{100, 100, 100, 100, 100}`);
        assert.contains(body, `  curve after["a41c9e2"]{50, 100, 300, 300, 100}`);
        assert.contains(body, "  max 300")
    }

    test "radar leaves room above the base" {
        let comparison = Comparison.new(
            report_of([fn_report("a", cognitive = 4n)]),
            report_of([fn_report("a", cognitive = 8n)]),
        );
        assert.contains(render(comparison, test_context()), "  max 150")
    }

    test "no radar when no average moved or the base had no functions" {
        let same = report_of([fn_report("a", cognitive = 3n)]);
        assert.is_true(!render(Comparison.new(same, same), test_context()).includes("mermaid"));
        assert.is_true(
            !render(Comparison.new(report_of([]), same), test_context()).includes("mermaid"),
        )
    }
}

testset "significance" {
    test "npath must double" {
        assert.is_true(
            !(MetricChange { metric: Metric.NPath, before: 8n, after: 12n }).significant(),
        );
        assert.is_true(
            (MetricChange { metric: Metric.NPath, before: 8n, after: 16n }).significant(),
        );
        assert.is_true(
            !(MetricChange { metric: Metric.NPath, before: 1n, after: 2n }).significant(),
        );
        assert.is_true(
            (MetricChange { metric: Metric.NPath, before: 36n, after: 4n }).significant(),
        )
    }

    test "nesting and exits need two" {
        assert.is_true(
            !(MetricChange { metric: Metric.Nesting, before: 1n, after: 2n }).significant(),
        );
        assert.is_true((MetricChange { metric: Metric.Exits, before: 3n, after: 1n }).significant())
    }
}

testset "ratio" {
    test "kept in lowest terms so equality agrees with cmp" {
        assert.equal(Ratio.new(6n, 4n), Ratio { numerator: 3n, denominator: 2n });
        assert.equal(Ratio.new(0n, 7n), Ratio { numerator: 0n, denominator: 1n });
        assert.is_true(Ratio.new(1n, 3n) < Ratio.new(1n, 2n))
    }

    test "rejects a denominator below one" {
        let rejected = Ratio.new(1n, 0n) catch (e) {
            baml.errors.InvalidArgument => null
        };
        assert.equal(rejected, null)
    }

    test "percent rounds half up" {
        assert.equal(Ratio.new(1n, 8n).percent(), 13n);
        assert.equal(Ratio.new(7n, 2n).percent(), 350n)
    }
}

testset "context" {
    test "new trims trailing slashes and normalizes the prefix" {
        let ctx = Context.new(
            "https://github.com//",
            "acme/app",
            "a41c9e2",
            "canary",
            "services\\agent\\",
            ".",
        );
        assert.equal(ctx.server_url, "https://github.com");
        assert.equal(ctx.project_prefix, "services/agent/")
    }
}

