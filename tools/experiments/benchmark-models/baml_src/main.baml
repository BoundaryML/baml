// baml-bench-experiment — benchmark current OpenAI and Anthropic models on
// writing BAML with describe-only documentation, graded by intentionbench.
//
// Pack and run:
//
//   baml pack Main -o baml-bench-experiment --output-format debug
//   ./baml-bench-experiment all                    # every model
//   ./baml-bench-experiment gpt-5.5 claude-sonnet-5
//   ./baml-bench-experiment openai                 # one provider's lineup
//
// Environment:
//   OPENAI_API_KEY / ANTHROPIC_API_KEY  builder credentials
//   LLM_BASE_URL / LLM_API_KEY          intentionbench grader (must be the
//                                       local subscription proxy)
//   INTENTIONBENCH_BIN                  optional path override
//
// Per-model flow: reserve worst-case budget -> self-exec a worker process
// (so the env-driven builder client binds to exactly one model) -> the worker
// runs the describe-only agent loop -> independent `baml check` + `baml test`
// -> intentionbench grades the generated project -> aggregate results.json.

// ---- results -------------------------------------------------------------------

class ResultRow {
    provider: string,
    model: string,
    compile_ok: bool,
    tests_ok: bool,
    turns_used: int,
    describes: int,
    writes: int,
    llm_errors: int,
    finished: bool,
    adherence: float?,
    validity_adjusted: float,
    commission: float?,
    omission: float?,
    graded_pairs: int?,
    slop: int?,
    run_dir: string,
}

// The slice of intentionbench's report.json this experiment consumes.
// (from_string tolerates the report's many extra fields.)
class BenchFinding {
    verdict: string,
    evidence_verified: bool,
    refuted: bool,
}

class BenchReport {
    adherence_score: float,
    commission_score: float,
    omission_score: float,
    graded_pairs: int,
    findings: BenchFinding[],
}

// Verified, unrefuted fighting/reinventing findings — the slop count.
function count_slop(report: BenchReport) -> int {
    let n = 0;
    for (let f in report.findings) {
        if (
            (f.verdict == "fighting" || f.verdict == "reinventing")
                && f.evidence_verified
                && !f.refuted
        ) {
            n = n + 1;
        }
    }
    n
}

// One-shot index dispenser for the parallel build tasks. take() has no await
// point, so the read-increment is atomic under cooperative scheduling.
class BuildQueue {
    next: int,

    function take(self, limit: int) -> int {
        if (self.next >= limit) {
            return -1;
        }
        let i = self.next;
        self.next = self.next + 1;
        i
    }
}

// ---- process helpers -----------------------------------------------------------

// ProcessOptions.env REPLACES the environment, so every subprocess needs the
// basics threaded through explicitly.
function base_env() -> map<string, string> {
    let env: map<string, string> = {};
    for (let name in ["PATH", "HOME", "TMPDIR"]) {
        match (
            baml.env.get(name) catch (_) {
                _ => null
            }
        ) {
            let v: string => {
                env.set(name, v);
            },
            null => null,
        }
    }
    env
}

function env_or_empty(name: string) -> string {
    (
        baml.env.get(name) catch (_) {
            _ => null
        }
    )
        ?? ""
}

function run_logged(
    program: string,
    args: string[],
    cwd: string?,
    env: map<string, string>?,
    timeout_ms: int,
    log_path: string,
) -> baml.sys.ShellOutput? {
    let out = baml
        .sys
        .exec(
            program,
            args,
            baml.sys.ProcessOptions { cwd: cwd, env: env, timeout_ms: timeout_ms, stdin: null },
        ) catch (e) {
        _ => {
            baml.fs.write(log_path, "exec failed: " + baml.unstable.string(e)) catch (_) {
                _ => null
            };
            return null;
        }
    };
    baml.fs.write(log_path, "$ " + program + " " + args.join(" ") + "\n" + command_text(out)) catch (_) {
        _ => null
    };
    out
}

// ---- per-model stages ----------------------------------------------------------

// Build one model's project by re-exec'ing this binary in worker mode with the
// BENCH_* env bound to that model. Returns true when the worker ran at all.
function build_project(self_bin: string, m: ModelSpec, project: string, run_dir: string) -> bool {
    let env = base_env();
    env.set("BENCH_MODEL", m.name);
    env.set("BENCH_BASE_URL", m.base_url);
    env.set("BENCH_API_KEY", env_or_empty(m.key_env));
    let out = run_logged(
        self_bin,
        ["__worker", m.provider, m.name, project, run_dir],
        null,
        env,
        max_turns() * 660000,
        run_dir + "/build.log",
    );
    match (out) {
        let o: baml.sys.ShellOutput => o.ok(),
        null => false,
    }
}

// Independent compile/test verdicts on the generated project. The builder had
// no compiler, so this is the first time the code meets one.
function validate_project(project: string, run_dir: string) -> bool[] {
    let checked = run_logged("baml", ["check"], project, base_env(), 120000, run_dir + "/check.log");
    let compile_ok = match (checked) {
        let o: baml.sys.ShellOutput => o.ok(),
        null => false,
    };
    if (!compile_ok) {
        return [false, false];
    }
    let tested = run_logged("baml", ["test"], project, base_env(), 300000, run_dir + "/test.log");
    let tests_ok = match (tested) {
        let o: baml.sys.ShellOutput => o.ok(),
        null => false,
    };
    [compile_ok, tests_ok]
}

function grade_project(bench_bin: string, project: string, run_dir: string) -> BenchReport? {
    let env = base_env();
    env.set("LLM_BASE_URL", env_or_empty("LLM_BASE_URL"));
    env.set("LLM_API_KEY", env_or_empty("LLM_API_KEY"));
    let out_dir = run_dir + "/bench";
    let out = run_logged(bench_bin, ["--target", project, "--out", out_dir], null, env, 1800000, run_dir + "/grade.log");
    match (out) {
        let o: baml.sys.ShellOutput => {
            if (!o.ok()) {
                return null;
            }
        },
        null => {
            return null;
        },
    }
    let text = baml.fs.read(out_dir + "/report.json") catch (_) {
        _ => ""
    };
    if (text == "") {
        return null;
    }
    baml.json.from_string<BenchReport>(text) catch (_) {
        _ => null
    }
}

function load_stats(run_dir: string) -> BuildStats? {
    let text = baml.fs.read(run_dir + "/build.json") catch (_) {
        _ => ""
    };
    if (text == "") {
        return null;
    }
    baml.json.from_string<BuildStats>(text) catch (_) {
        _ => null
    }
}

// ---- report --------------------------------------------------------------------

function score_cell(v: float?) -> string {
    match (v) {
        let x: float => f2(x),
        null => "n/a",
    }
}

function print_table(rows: ResultRow[]) -> null {
    let header = rpad("model", 18)
        + pad("check", 6)
        + pad("tests", 6)
        + pad("turns", 6)
        + pad("adh", 7)
        + pad("valid", 7)
        + pad("comm", 7)
        + pad("omit", 7)
        + pad("slop", 6);
    baml.io.println("");
    baml.io.println(header);
    let dashes = "";
    while (dashes.length() < header.length()) {
        dashes = dashes + "-";
    }
    baml.io.println(dashes);
    let sorted = rows
        .sort_by_key(
            (r: ResultRow) -> int {
                let key = (r.validity_adjusted * -100.0).iround() catch (_) {
                    _ => 0
                };
                key
            },
        );
    for (let r in sorted) {
        baml.io
            .println(
                rpad(r.model, 18)
                    + pad(baml.unstable.string(r.compile_ok), 6)
                    + pad(baml.unstable.string(r.tests_ok), 6)
                    + pad(i2s(r.turns_used), 6)
                    + pad(score_cell(r.adherence), 7)
                    + pad(f2(r.validity_adjusted), 7)
                    + pad(score_cell(r.commission), 7)
                    + pad(score_cell(r.omission), 7)
                    + pad(
                        match (r.slop) {
                            let s: int => i2s(s),
                            null => "n/a",
                        },
                        6,
                    ),
            );
    }
    null
}

// ---- entry ---------------------------------------------------------------------

function Main() -> int {
    let argv = baml.sys.argv();
    // Packed argv shape: [binary, "Main", user args…]
    if (argv.length() > 2 && argv.at(2) == "__worker") {
        let provider = argv.at(3) ?? "";
        let model = argv.at(4) ?? "";
        let project = argv.at(5) ?? "";
        let run_dir = argv.at(6) ?? "";
        if (provider == "" || model == "" || project == "" || run_dir == "") {
            baml.io.eprintln("__worker needs: provider model project_dir run_dir");
            return 2;
        }
        return run_worker(provider, model, project, run_dir);
    }

    let args: string[] = [];
    let i = 2;
    while (i < argv.length()) {
        args.push(argv.at(i) ?? "");
        i = i + 1;
    }
    let selected = resolve_models(args);

    // Fail fast on missing credentials rather than 20 turns into a build.
    for (let m in selected) {
        if (env_or_empty(m.key_env) == "") {
            baml.io.eprintln("missing " + m.key_env + " (needed for " + m.name + ")");
            return 2;
        }
    }
    let grader_url = env_or_empty("LLM_BASE_URL");
    if (grader_url == "" || env_or_empty("LLM_API_KEY") == "") {
        baml.io.eprintln("set LLM_BASE_URL and LLM_API_KEY for the intentionbench grader");
        return 2;
    }
    if (!grader_url.starts_with("http://localhost:") && !grader_url.starts_with("http://127.0.0.1:")) {
        baml.io
            .eprintln(
                "hard budget stop: the grader must point at the local subscription proxy; "
                    + "a remote grader would add unmetered API spend",
            );
        return 2;
    }
    let bench_bin = env_or_empty("INTENTIONBENCH_BIN");
    if (bench_bin == "") {
        bench_bin = "../../baml-bench/intentionbench";
    }
    if (!baml.fs.exists(bench_bin)) {
        baml.io
            .eprintln(
                "intentionbench binary not found at "
                    + bench_bin
                    + " — build it with `baml pack Bench -o intentionbench --output-format debug` in tools/baml-bench, "
                    + "or set INTENTIONBENCH_BIN",
            );
        return 2;
    }
    let self_bin = argv.at(0) ?? "";

    // Reserve worst-case budget for every build we are about to start.
    // Projects that already exist are re-used without a new reservation.
    let ledger = load_ledger("budget.json");
    let to_build: map<string, bool> = {};
    for (let m in selected) {
        let project = "runs/" + m.name + "/project";
        if (baml.fs.exists(project + "/baml_src")) {
            baml.io.println("== " + m.name + ": re-using existing project ==");
            continue;
        }
        ledger
            = reserve(ledger, "budget.json", m) catch (e) {
                BudgetExceeded => {
                    baml.io.eprintln(e.message);
                    baml.sys.exit(2)
                }
            };
        to_build.set(m.name, true);
        baml.io
            .println(
                "== "
                    + m.name
                    + ": reserved worst-case $"
                    + f2(worst_case_cost(m))
                    + " (ledger $"
                    + f2(ledger.reserved_usd)
                    + " / $"
                    + f2(budget_limit_usd())
                    + ") ==",
            );
    }

    // Builds run in parallel — each worker is an independent subprocess bound
    // to one model. Grading stays sequential because intentionbench already
    // saturates the local grader proxy with its own worker pools.
    //
    // Spawn closures must not capture loop variables (they can be re-read
    // after the loop has moved on), so each task claims an index from a
    // shared queue and reads the immutable build list inside the closure.
    let build_list: ModelSpec[] = [];
    for (let m in selected) {
        if (to_build.has(m.name)) {
            build_list.push(m);
            baml.fs.mkdir("runs/" + m.name + "/project", baml.fs.MkdirOptions { recursive: true }) catch (_) {
                _ => null
            };
        }
    }
    let queue = BuildQueue { next: 0 };
    let handles = [];
    let w = 0;
    while (w < build_list.length()) {
        handles
            .push(
                spawn {
                    // idx is never -1 here (one task per list entry); at(-1)
                    // would fall through to the null arm anyway.
                    let idx = queue.take(build_list.length());
                    match (build_list.at(idx)) {
                        let spec: ModelSpec => {
                            let run_dir = "runs/" + spec.name;
                            let ok = build_project(self_bin, spec, run_dir + "/project", run_dir);
                            baml.io
                                .eprintln(
                                    "[build done] " + spec.name + " ok=" + baml.unstable.string(ok),
                                );
                            ok
                        },
                        null => false,
                    }
                },
            );
        w = w + 1;
    }
    for (let h in handles) {
        await h;
    }

    let rows: ResultRow[] = [];
    for (let m in selected) {
        let run_dir = "runs/" + m.name;
        let project = run_dir + "/project";
        baml.io.println("== " + m.name + ": check + test + grade ==");
        let verdicts = validate_project(project, run_dir);
        let compile_ok = verdicts.at(0) ?? false;
        let tests_ok = verdicts.at(1) ?? false;
        let report = grade_project(bench_bin, project, run_dir);
        let stats = load_stats(run_dir);
        let adherence: float? = match (report) {
            let r: BenchReport => r.adherence_score,
            null => null,
        };
        rows.push(
            ResultRow {
                provider: m.provider,
                model: m.name,
                compile_ok: compile_ok,
                tests_ok: tests_ok,
                turns_used: match (stats) {
                    let s: BuildStats => s.turns_used,
                    null => 0,
                },
                describes: match (stats) {
                    let s: BuildStats => s.describes,
                    null => 0,
                },
                writes: match (stats) {
                    let s: BuildStats => s.writes,
                    null => 0,
                },
                llm_errors: match (stats) {
                    let s: BuildStats => s.llm_errors,
                    null => 0,
                },
                finished: match (stats) {
                    let s: BuildStats => s.finished,
                    null => false,
                },
                adherence: adherence,
                // Idiomatic-looking code that does not compile must not win.
                validity_adjusted: if (compile_ok) {
                    adherence ?? 0.0
                } else {
                    0.0
                },
                commission: match (report) {
                    let r: BenchReport => r.commission_score,
                    null => null,
                },
                omission: match (report) {
                    let r: BenchReport => r.omission_score,
                    null => null,
                },
                graded_pairs: match (report) {
                    let r: BenchReport => r.graded_pairs,
                    null => null,
                },
                slop: match (report) {
                    let r: BenchReport => count_slop(r),
                    null => null,
                },
                run_dir: run_dir,
            },
        );
        baml.fs
            .write(
                "results.json",
                baml.json.to_string(rows) catch (_) {
                    _ => "[]"
                },
            ) catch (_) {
            _ => null
        };
    }

    print_table(rows);
    baml.io.println("\nresults: results.json  runs: runs/<model>/");
    0
}

test "slop counting" {
    let report = BenchReport {
        adherence_score: 5.0,
        commission_score: 5.0,
        omission_score: 1.0,
        graded_pairs: 3,
        findings: [
            BenchFinding { verdict: "fighting", evidence_verified: true, refuted: false },
            BenchFinding { verdict: "reinventing", evidence_verified: true, refuted: true },
            BenchFinding { verdict: "fighting", evidence_verified: false, refuted: false },
            BenchFinding { verdict: "adherent", evidence_verified: true, refuted: false },
        ],
    };
    assert.equal(count_slop(report), 1)
}

test "bench report decodes with extra fields" {
    let raw = #"
        {
            "target": "x", "prompt_version": "v1", "block_count": 10,
            "adherence_score": 6.5, "commission_score": 7.0, "omission_score": 0.9,
            "graded_pairs": 42,
            "findings": [
                {"block_id": "F:a", "verdict": "fighting", "evidence_verified": true,
                 "refuted": false, "grade": 3, "weight": 1.0}
            ]
        }
    "#;
    let report = baml.json.from_string<BenchReport>(raw);
    assert.equal(report.graded_pairs, 42);
    assert.equal(count_slop(report), 1)
}
