// The triage pipeline: feedback in -> (already fixed | enriched ticket) out.
//
//   extract_repro (LLM) -> run_repro (in-process reflect compile) -> enrich_ticket (LLM)
//
// Repros are checked with baml.reflect.Package.compile — no subprocess, no VM
// shellout. A repro carries an Expectation describing the CORRECT behavior;
// if the current toolchain matches it, the bug is already fixed.

// The locally-authenticated Claude Code CLI — no API key needed.
client Triage = claude_code.ClaudeCodeClient.new(model = "sonnet", timeout_ms = 600000);

// ---------- Stage 1: minimal repro (LLM) ----------

// The `check` literal on each Expectation variant is a WIRE discriminator,
// not for code (code matches on the class). Without it the union is
// unparseable: the parser takes the first structurally-matching arm, an
// empty/all-optional class matches any object, so every LLM reply would
// decode as ShouldCompile. The language has no field defaults, hence the
// tag is spelled at each construction site.

/// The code should compile cleanly (bug: valid code is rejected).
class ShouldCompile {
    check: "should_compile",
}

/// The code should be rejected (bug: invalid code is accepted).
class ShouldNotCompile {
    check: "should_not_compile",
    /// Substring the compiler diagnostic should contain, if the report names one.
    diagnostic_contains: string?,
}

/// `repro_main()` should evaluate to `expected` (bug: wrong result at runtime).
/// The repro files must export `function repro_main() -> json`.
class ShouldEvaluateTo {
    check: "should_evaluate_to",
    expected: json,
}

/// No programmatic check exists (needs a human to look at output, IDE, etc.).
class RequiresInspection {
    check: "requires_inspection",
    instructions: string,
}

type Expectation = ShouldCompile | ShouldNotCompile | ShouldEvaluateTo | RequiresInspection;

class Repro {
    /// BAML sources, keyed by file name (e.g. "main.baml") — the complete
    /// project needed by `command` (generator blocks, baml.toml, ...).
    files: map<string, string>,
    /// The exact baml invocation that demonstrates the bug, mirroring the
    /// issue-style repro: "baml run main.baml", "baml generate", "baml test",
    /// "baml fmt main.baml", ...
    command: string,
    /// Anything needed beyond files + command (env vars, platform); null if
    /// self-contained.
    setup: string?,
    expectation: Expectation,
}

/// Turn raw feedback into a minimal, self-contained repro.
function extract_repro(fb: Feedback) -> Repro {
    client: Triage
    prompt: `
        You are triaging a bug report for BAML, a statically-typed language with
        first-class LLM functions. Distill the report into a MINIMAL repro of
        the kind a maintainer would paste into the issue tracker: project
        files plus the exact baml command that breaks.

        Rules:
        - "command" is the baml invocation that demonstrates the bug:
          "baml run main.baml", "baml generate", "baml test", "baml fmt ...".
          Pick whatever command the report says (or implies) is broken — the
          repro does NOT have to be a pure language snippet. For a codegen bug
          include the generator block in the files and use "baml generate".
        - "files" maps file names to contents — everything the command needs,
          nothing more. Strip anything not needed to trigger the bug; no LLM
          clients or prompts unless the bug is about them.
        - Pick the expectation describing CORRECT behavior:
          * should_compile - the report says valid code is wrongly rejected.
          * should_not_compile - invalid code is wrongly accepted. Set
            diagnostic_contains only if the report names the expected error text.
          * should_evaluate_to - code runs but produces a wrong value. The files
            MUST export a zero-argument "function repro_main() -> json" returning
            the value under test, and "expected" is the CORRECT value.
          * requires_inspection - the command's outcome needs a human (or a
            host toolchain) to evaluate: codegen output quality, formatting,
            IDE behavior, performance, billing. STILL provide the files and
            command; "instructions" says what to look at after running it.
        - "setup" is null unless env vars or flags are genuinely required.

        Report title: ${fb.title}
        Report body:
        ${fb.body}

        Comment chain:
        ${format_comments(fb.comments)}

        ${ctx.output_format}
    `
}

// ---------- Stage 2: check if already fixed (in-process) ----------

enum Verdict {
    /// Current toolchain still violates the expectation — the bug reproduces.
    StillBroken,
    /// Current toolchain matches the expectation — already fixed.
    Fixed,
    /// Repro requires inspection or the check itself could not run.
    Inconclusive,
}

class ReproRun {
    verdict: Verdict,
    /// What we actually observed (diagnostic text, evaluated value, ...).
    observed: string,
}

class CompileAttempt {
    pkg: baml.reflect.Package?,
    error: string?,
    /// True when the host running us has no runtime compiler (e.g. the
    /// `baml test` VM) — the check is impossible, not failed.
    unavailable: bool,
}

function try_compile(files: map<string, string>) -> CompileAttempt {
    compile_ok(files) catch (e) {
        baml.reflect.errors.CompilationError => {
            CompileAttempt { pkg: null, error: e.message, unavailable: false }
        },
        baml.errors.Unsupported => CompileAttempt { pkg: null, error: e.message, unavailable: true },
        _ => {
            CompileAttempt {
                pkg: null,
                error: "compilation failed for an unexpected reason",
                unavailable: false,
            }
        },
    }
}

function compile_ok(files: map<string, string>) -> CompileAttempt throws unknown {
    let pkg = baml.reflect.Package.compile(files);
    CompileAttempt { pkg: pkg, error: null, unavailable: false }
}

type ReproMain = () -> json throws unknown;

function repro_main_output(pkg: baml.reflect.Package) -> string {
    let f = pkg.get_function<ReproMain>("repro_main") catch (_) {
        _ => null,
    };
    if (f == null) {
        return "error: no `function repro_main() -> json` exported";
    };
    baml.json.stringify(
        f() catch (_) {
            _ => "error: repro_main threw"
        },
    )
}

/// Run a repro against the toolchain executing this program.
function run_repro(repro: Repro) -> ReproRun {
    match (repro.expectation) {
        let e: RequiresInspection => {
            ReproRun {
                verdict: Verdict.Inconclusive,
                observed: "requires inspection: " + e.instructions,
            }
        },
        let e: ShouldCompile => {
            let attempt = try_compile(repro.files);
            if (attempt.unavailable) {
                return ReproRun {
                    verdict: Verdict.Inconclusive,
                    observed: "runtime compiler unavailable in this host: " + (attempt.error ?? ""),
                };
            };
            match (attempt.error) {
                null => ReproRun { verdict: Verdict.Fixed, observed: "compiles cleanly" },
                let msg: string => {
                    ReproRun { verdict: Verdict.StillBroken, observed: "still rejected: " + msg }
                },
            }
        },
        let e: ShouldNotCompile => {
            let attempt = try_compile(repro.files);
            if (attempt.unavailable) {
                return ReproRun {
                    verdict: Verdict.Inconclusive,
                    observed: "runtime compiler unavailable in this host: " + (attempt.error ?? ""),
                };
            };
            match (attempt.error) {
                null => {
                    ReproRun {
                        verdict: Verdict.StillBroken,
                        observed: "still accepted (compiles cleanly)",
                    }
                },
                let msg: string => {
                    let wanted = e.diagnostic_contains;
                    if (wanted != null && !msg.includes(wanted ?? "")) {
                        return ReproRun {
                            verdict: Verdict.Inconclusive,
                            observed: "rejected, but diagnostic differs: " + msg,
                        };
                    };
                    ReproRun { verdict: Verdict.Fixed, observed: "now rejected: " + msg }
                },
            }
        },
        let e: ShouldEvaluateTo => {
            let attempt = try_compile(repro.files);
            if (attempt.unavailable) {
                return ReproRun {
                    verdict: Verdict.Inconclusive,
                    observed: "runtime compiler unavailable in this host: " + (attempt.error ?? ""),
                };
            };
            match (attempt.pkg) {
                null => {
                    ReproRun {
                        verdict: Verdict.Inconclusive,
                        observed: "repro no longer compiles: " + (attempt.error ?? "unknown error"),
                    }
                },
                let pkg: baml.reflect.Package => {
                    let observed = repro_main_output(pkg);
                    let expected = baml.json.stringify(e.expected);
                    if (observed == expected) {
                        ReproRun {
                            verdict: Verdict.Fixed,
                            observed: "evaluates to expected " + observed,
                        }
                    } else {
                        ReproRun {
                            verdict: Verdict.StillBroken,
                            observed: "expected " + expected + " but got " + observed,
                        }
                    }
                },
            }
        },
    }
}

// ---------- Stage 3: enriched ticket (LLM) ----------

/// The LLM-authored parts of a ticket; ids, status, repros are assembled
/// deterministically in `assemble_issue`.
class IssueDraft {
    title: string,
    description: string,
    subsystem: Subsystem,
    resolution_plan: string?,
}

/// Write a fully enriched ticket for a bug confirmed (or unconfirmed) on the
/// current toolchain.
function enrich_ticket(fb: Feedback, repro: Repro, run: ReproRun) -> IssueDraft {
    client: Triage
    prompt: `
        Write an engineering ticket for the BAML compiler team from this bug
        report and its verified minimal repro.

        - title: one imperative line naming the defect precisely.
        - description: symptoms, the minimal repro's essence, what we observed
          when running it, and expected vs actual behavior. Written for an
          engineer who has not read the original report.
        - subsystem: where the defect most likely lives.
          syntax = parsing/grammar; compiler = type checking, MIR, diagnostics,
          ICEs; runtime = the VM, serde, SDK runtime behavior; std_library =
          builtin functions/types; tooling = CLI, code generators for host
          languages (sdkgen/codegen), IDE/playground, packaging, CI.
        - resolution_plan: a short outline of a plausible fix (files/phases to
          look at, suggested approach), or null if you cannot say anything
          non-obvious.

        Original report: ${fb.title}
        ${fb.body}

        Minimal repro files:
        ${format_files(repro.files)}

        Checker verdict on the current toolchain: ${run.verdict.to_string()}
        Observed: ${run.observed}

        ${ctx.output_format}
    `
}

// ---------- Orchestration ----------

class TriageResult {
    feedback_id: string,
    verdict: Verdict,
    observed: string,
    /// Null when the bug is already fixed on the current toolchain.
    issue: Issue?,
}

function toolchain_version() -> string {
    baml.env.get("BAML_VERSION") ?? "unknown"
}

/// One piece of feedback, reproduced: the minimal repro plus what happened
/// when we ran it on this toolchain.
class Reproduction {
    repro: Repro,
    run: ReproRun,
}

/// PIPELINE STAGE 1: create a minimal repro from the feedback and run it
/// in-process to see whether the bug still exists.
function reproduce(fb: Feedback) -> Reproduction throws unknown {
    let repro = extract_repro(fb);
    let run = run_repro(repro);
    log.info({ "stage": "repro", "verdict": run.verdict.to_string(), "observed": run.observed });
    Reproduction { repro: repro, run: run }
}

/// Assemble a full Issue from its LLM-drafted parts. Deterministic — split
/// from create_ticket so tests can exercise it with a cached draft.
function assemble_issue(fb: Feedback, r: Reproduction, draft: IssueDraft) -> Issue {
    Issue {
        id: "ISSUE-" + baml.time.Instant.now().to_timestamp_milliseconds().to_string(),
        title: draft.title,
        description: draft.description,
        shepherd: null,
        subsystem: draft.subsystem,
        repros: [r.repro],
        version: toolchain_version(),
        feedback_ids: [fb.id],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: draft.resolution_plan,
    }
}

/// PIPELINE STAGE 2: write the fully enriched ticket for a reproduction that
/// did not come back Fixed.
function create_ticket(fb: Feedback, r: Reproduction) -> Issue throws unknown {
    assemble_issue(fb, r, enrich_ticket(fb, r.repro, r.run))
}

/// The whole flow: reproduce, then ticket unless already fixed.
function triage(fb: Feedback) -> TriageResult throws unknown {
    log.info({ "stage": "start", "feedback": fb.id, "reporter": author_identity(fb.author) });
    let r = reproduce(fb);
    TriageResult {
        feedback_id: fb.id,
        verdict: r.run.verdict,
        observed: r.run.observed,
        issue: match (r.run.verdict) {
            Verdict.Fixed => null,
            _ => create_ticket(fb, r),
        },
    }
}

/// CLI entrypoint: `baml run -- <feedback.json> [out_dir]`
function main() -> null {
    let args = baml.sys.argv();
    let path = args.at(1)
        ?? {
            baml.sys.panic("usage: baml run -- <feedback.json> [out_dir]");
            ""
        };
    let out_dir = args.at(2) ?? "out";

    let fb = baml.json.from_string<Feedback>(baml.fs.read(path));
    let result = triage(fb) catch (e) {
        InvalidFeedback => {
            baml.sys.panic("invalid feedback: " + path);
            TriageResult {
                feedback_id: fb.id,
                verdict: Verdict.Inconclusive,
                observed: "",
                issue: null,
            }
        },
    };

    match (result.issue) {
        null => {
            log.info(
                {
                    "outcome": "already_fixed",
                    "on": toolchain_version(),
                    "observed": result.observed,
                },
            )
        },
        let issue: Issue => {
            let out_path = out_dir + "/" + issue.id + ".json";
            baml.fs.write(out_path, baml.json.stringify_pretty(issue.to_json()));
            log.info({ "outcome": "ticket_created", "path": out_path, "title": issue.title });
        },
    }
    null
}

// ---------- Prompt formatting helpers ----------

function format_comments(comments: Comment[]) -> string {
    comments
        .map((c: Comment) -> string {
            "- " + c.author + ": " + c.body
        })
        .join("\n")
}

function format_repro(r: Repro) -> string {
    "$ " + r.command + "\n\n" + format_files(r.files)
}

function format_files(files: map<string, string>) -> string {
    files.keys()
        .map((name: string) -> string {
            "--- " + name + " ---\n" + (files.get(name) ?? "")
        })
        .join("\n\n")
}

// ==================== Eval scoring ====================
//
// The eval DATASET and its generator are not checked in: the data lives in
// gitignored eval/, the generator in gitignored baml_src/evalgen.baml.
//
//   baml run -e 'build_eval_dataset(60, 3)'      # generator (gitignored file)
//   baml run -e 'run_eval()'                     # feedback->issue reproduction metrics
//   baml run -e 'run_enrichment_eval()'          # enrich_ticket-only metrics

class IssueTruth {
    title: string,
    fixed: bool,
    subsystem: Subsystem,
    /// The repro the maintainer put in the GitHub issue (first code fence).
    /// Every properly-formatted issue in the test database has one.
    reference_repro: string?,
}

class GroundTruth {
    issues: map<string, IssueTruth>,
    feedback_to_issue: map<string, int>,
    duplicate_groups: string[][],
}

function load_ground_truth(dir: string = "eval") -> GroundTruth {
    baml.json.from_string<GroundTruth>(baml.fs.read(dir + "/ground_truth.json"))
}

/// LLM/network calls can fail transiently — run `f` up to `attempts` times
/// with a short pause between tries; null when every attempt failed.
function retry<T>(attempts: int, f: () -> T throws unknown) -> T? {
    let tries = 0;
    while (tries < attempts) {
        tries += 1;
        let r = f() catch (_) {
            _ => null,
        };
        if (r != null) {
            return r;
        };
        baml.sys.sleep(baml.time.Duration.from_seconds(1)) catch (_) {
            _ => null,
        };
    }
    null
}

// ---------- Metrics (plain BAML — rates and means over named counters) ----------

class Metrics {
    hits: map<string, int>,
    totals: map<string, int>,
    sums: map<string, float>,

    function new() -> Metrics {
        Metrics { hits: {}, totals: {}, sums: {} }
    }

    /// Record a pass/fail observation; reported as a rate in [0,1].
    function hit(self, name: string, ok: bool) -> null {
        self.totals.set(name, (self.totals.get(name) ?? 0) + 1);
        if (ok) {
            self.hits.set(name, (self.hits.get(name) ?? 0) + 1);
        };
        null
    }

    /// Record a numeric observation; reported as a mean.
    function add(self, name: string, v: float) -> null {
        self.totals.set(name, (self.totals.get(name) ?? 0) + 1);
        self.sums.set(name, (self.sums.get(name) ?? 0.0) + v);
        null
    }

    /// name -> rate (hit metrics) or mean (add metrics), plus name.n -> count.
    function report(self) -> map<string, float> {
        let out: map<string, float> = {};
        for (let name in self.totals.keys()) {
            let n = self.totals.get(name) ?? 0;
            let sum = self.sums.get(name);
            let value = if (sum != null) {
                (sum ?? 0.0) / (1.0 * n)
            } else {
                (1.0 * (self.hits.get(name) ?? 0)) / (1.0 * n)
            };
            out.set(name, value);
            out.set(name + ".n", 1.0 * n);
        }
        out
    }
}

// ---------- Repro quality judge (LLM) ----------

class ReproJudgment {
    /// 0.0 (unrelated) .. 1.0 (equivalent trigger for the same defect).
    similarity: float,
    /// What our repro misses relative to the reference, if anything.
    missing: string?,
}

/// Judge whether OUR generated repro exercises the same defect as the
/// maintainer's reference repro from the GitHub issue.
function judge_repro(ours: string, reference: string) -> ReproJudgment {
    client: Triage
    prompt: `
        Two bug reproductions for the same BAML toolchain issue, each a baml
        command plus project files. Judge whether OURS triggers the same
        defect as the REFERENCE (the maintainer's own repro from the issue
        tracker). Similarity is about the defect being exercised — same
        command surface, same trigger — not formatting or naming:
        1.0 = same trigger, same expected failure; 0.7 = same defect, weaker
        or partial trigger; 0.4 = plausibly related; 0.0 = different bug.

        REFERENCE:
        ${reference}

        OURS:
        ${ours}

        ${ctx.output_format}
    `
}

// ---------- Feedback -> issue repro-match eval ----------
//
// THE metric: from each synthetic feedback, does extract_repro produce the
// same repro the maintainer put in the GitHub issue? Nothing is executed —
// judge_repro compares our (command + files) against the reference.

class EvalRow {
    feedback_id: string,
    issue_number: int,
    command: string,
    /// judge_repro similarity vs the issue's reference repro.
    similarity: float?,
    /// similarity >= 0.7 — same defect exercised.
    matched: bool,
    /// what our repro misses relative to the reference, per the judge.
    missing: string?,
}

/// Resumable: rows persist per feedback; rerunning skips scored rows.
function run_eval(dir: string = "eval") -> null {
    let gt = load_ground_truth(dir = dir);
    let rows: EvalRow[] = if (baml.fs.exists(dir + "/results-partial.json")) {
        baml.json.from_string<EvalRow[]>(baml.fs.read(dir + "/results-partial.json"))
    } else {
        []
    };
    let done: map<string, bool> = {};
    for (let row in rows) {
        done.set(row.feedback_id, true);
    }

    for (let fid in gt.feedback_to_issue.keys()) {
        if (done.has(fid)) {
            continue;
        };
        let issue_number = gt.feedback_to_issue.get(fid) ?? 0;
        let truth = gt.issues.get(issue_number.to_string())
            ?? baml.sys.panic("no ground truth for issue " + issue_number.to_string());
        let reference = truth.reference_repro ?? "";

        let fb = baml.json.from_string<Feedback>(baml.fs.read(dir + "/feedback/" + fid + ".json"));
        let repro = retry<Repro>(3, () -> Repro throws unknown {
            extract_repro(fb)
        });
        let judged = if (repro == null) {
            null
        } else {
            retry<ReproJudgment>(2, () -> ReproJudgment throws unknown {
                judge_repro(format_repro(repro ?? baml.sys.panic("unreachable")), reference)
            })
        };

        rows.push(
            EvalRow {
                feedback_id: fid,
                issue_number: issue_number,
                command: repro?.command ?? "<extract failed>",
                similarity: judged?.similarity,
                matched: (judged?.similarity ?? 0.0) >= 0.7,
                missing: judged?.missing,
            },
        );
        baml.fs.write(dir + "/results-partial.json", baml.json.stringify_pretty(rows.to_json()));
        log.info({ "scored": fid, "similarity": judged?.similarity });
    }

    let m = Metrics.new();
    for (let row in rows) {
        m.hit("extracted", row.similarity != null);
        m.hit("repro_match", row.matched);
        let s = row.similarity;
        if (s != null) {
            m.add("similarity", s ?? 0.0);
        };
    }
    let summary = { "metrics": m.report().to_json(), "rows": rows.to_json() };
    baml.fs.write(dir + "/results.json", baml.json.stringify_pretty(summary.to_json()));
    null
}

// ---------- Enrichment-only eval ----------
// Scores JUST enrich_ticket: can it autopopulate a good ticket from feedback
// alone? The repro/checker stages are stubbed out so each row is one LLM call.
//   baml run -e 'run_enrichment_eval()'

class EnrichmentRow {
    feedback_id: string,
    issue_number: int,
    subsystem: Subsystem,
    subsystem_expected: Subsystem,
    subsystem_match: bool,
    ticket_title: string,
    description_len: int,
    has_resolution_plan: bool,
}

function run_enrichment_eval(dir: string = "eval") -> null {
    let gt = load_ground_truth(dir = dir);
    let m = Metrics.new();
    let rows: EnrichmentRow[] = [];

    for (let fid in gt.feedback_to_issue.keys()) {
        let issue_number = gt.feedback_to_issue.get(fid) ?? 0;
        let truth = gt.issues.get(issue_number.to_string())
            ?? baml.sys.panic("no ground truth for issue " + issue_number.to_string());

        let fb = baml.json.from_string<Feedback>(baml.fs.read(dir + "/feedback/" + fid + ".json"));
        let stub_repro = Repro {
            files: {},
            command: "baml run main.baml",
            setup: null,
            expectation: RequiresInspection { check: "requires_inspection", instructions: "not yet reproduced" },
        };
        let stub_run = ReproRun { verdict: Verdict.Inconclusive, observed: "repro not attempted" };

        let draft = retry<IssueDraft>(3, () -> IssueDraft throws unknown {
            enrich_ticket(fb, stub_repro, stub_run)
        });
        if (draft == null) {
            log.warn({ "feedback": fid, "skipped": "enrich failed after retries" });
            continue;
        };

        m.hit("subsystem_match", draft.subsystem == truth.subsystem);
        m.hit("has_resolution_plan", draft.resolution_plan != null);
        m.add("description_len", 1.0 * draft.description.length());

        rows.push(
            EnrichmentRow {
                feedback_id: fid,
                issue_number: issue_number,
                subsystem: draft.subsystem,
                subsystem_expected: truth.subsystem,
                subsystem_match: draft.subsystem == truth.subsystem,
                ticket_title: draft.title,
                description_len: draft.description.length(),
                has_resolution_plan: draft.resolution_plan != null,
            },
        );
        baml.fs.write("eval/enrichment-partial.json", baml.json.stringify_pretty(rows.to_json()));
    }

    let summary = { "metrics": m.report().to_json(), "rows": rows.to_json() };
    baml.fs.write(dir + "/enrichment-results.json", baml.json.stringify_pretty(summary.to_json()));
    null
}

// ==================== Tests ====================
// Token-free: parsing runs through the same machinery as live replies
// (from_string / Fn$parse). Checks needing the runtime compiler (absent in
// the `baml test` VM) live in `smoke()`: baml run -e 'smoke()'.

testset "feedback" {
    test "parses and picks the strongest identity" {
        let raw = #"
            {
              "id": "FB-1",
              "title": "match on enum broken",
              "body": "…",
              "source": "Github",
              "author": { "email": null, "github": "octocat", "device_id": "dev-9" },
              "comments": [],
              "issue_ids": []
            }
        "#;
        let fb = baml.json.from_string<Feedback>(raw);
        assert.equal(author_identity(fb.author), "octocat");
    }

    test "no identity throws InvalidFeedback" {
        let anon = Author { email: null, github: null, device_id: null };
        let got = author_identity(anon) catch (_) {
            InvalidFeedback => "rejected",
        };
        assert.equal(got, "rejected");
    }
}

testset "run_repro" {
    test "requires_inspection is inconclusive" {
        let r = Repro {
            files: {},
            setup: null,
            expectation: RequiresInspection { check: "requires_inspection", instructions: "look at fmt output" },
        };
        assert.equal(run_repro(r).verdict, Verdict.Inconclusive);
    }

    test "compiler-less host degrades to inconclusive, not a wrong verdict" {
        let r = Repro {
            files: { "main.baml": "function f() -> int { 41 + 1 }" },
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        };
        let run = run_repro(r);
        assert.equal(run.verdict, Verdict.Inconclusive);
        assert.contains(run.observed, "runtime compiler unavailable");
    }
}

testset "metrics" {
    test "rates and means aggregate correctly" {
        let m = Metrics.new();
        m.hit("ok", true);
        m.hit("ok", false);
        m.add("score", 0.5);
        m.add("score", 1.0);
        let report = m.report();
        assert.equal(report.get("ok") ?? -1.0, 0.5);
        assert.equal(report.get("ok.n") ?? -1.0, 2.0);
        assert.equal(report.get("score") ?? -1.0, 0.75);
    }
}

testset "llm_parsing" {
    test "extract_repro reply parses into a typed Repro" {
        let raw = #"
            {
              "files": { "main.baml": "function repro_main() -> json { [1,2].at(5) ?? -1 }" },
              "command": "baml run -e repro_main()",
              "setup": null,
              "expectation": { "check": "should_evaluate_to", "expected": -1 }
            }
        "#;
        let repro = extract_repro$parse(raw);
        assert.is_true(repro.files.has("main.baml"));
        match (repro.expectation) {
            let e: ShouldEvaluateTo => assert.equal(baml.json.stringify(e.expected), "-1"),
            _ => baml.sys.panic("expected should_evaluate_to"),
        }
    }

    test "enrich_ticket reply parses and builds an open issue" {
        let raw = #"
            {
              "title": "Array.at returns wrong element",
              "description": "…",
              "subsystem": "StdLibrary",
              "resolution_plan": "audit bounds check in array intrinsics"
            }
        "#;
        let draft = enrich_ticket$parse(raw);
        let fb = Feedback {
            id: "FB-2",
            title: "t",
            body: "b",
            source: FeedbackSource.BamlFeedback,
            author: Author { email: "a@b.c", github: null, device_id: null },
            comments: [],
            issue_ids: [],
        };
        let repro = Repro {
            files: {},
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        };
        let run = ReproRun { verdict: Verdict.StillBroken, observed: "still rejected" };
        let issue = assemble_issue(fb, Reproduction { repro: repro, run: run }, draft);
        assert.equal(issue.subsystem, Subsystem.StdLibrary);
        assert.contains(issue.id, "ISSUE-");
        match (issue.status) {
            let s: Open => assert.equal(s.state, "open"),
            _ => baml.sys.panic("expected open status"),
        }
    }

    test "judge_repro reply parses" {
        let raw = #"{ "similarity": 0.8, "missing": null }"#;
        let j = judge_repro$parse(raw);
        assert.equal(j.similarity, 0.8);
    }
}

// Dataset tests only exist once the (gitignored) dataset has been generated
// with build_eval_dataset — zero tests otherwise.
testset "eval_dataset" {
    if (baml.fs.exists("eval/ground_truth.json")) {
        let gt = load_ground_truth();
        for (let fid in gt.feedback_to_issue.keys()) {
            test ("parses " + fid) {
                let fb = baml.json.from_string<Feedback>(baml.fs.read("eval/feedback/" + fid + ".json"));
                assert.equal(fb.id, fid);
                assert.is_true(author_identity(fb.author).length() > 0);
            }
        }
        test "ground truth is internally consistent and properly formatted" {
            let gt2 = load_ground_truth();
            for (let group in gt2.duplicate_groups) {
                let first = gt2.feedback_to_issue.get(group.at(0) ?? "") ?? -1;
                for (let fid in group) {
                    assert.equal(gt2.feedback_to_issue.get(fid) ?? -2, first);
                }
            }
            for (let fid in gt2.feedback_to_issue.keys()) {
                let n = gt2.feedback_to_issue.get(fid) ?? 0;
                assert.is_true(gt2.issues.has(n.to_string()));
            }
            // every issue in the test database must be properly formatted:
            // a title and a non-empty reference repro from the issue body
            for (let key in gt2.issues.keys()) {
                let truth = gt2.issues.get(key);
                assert.is_true((truth?.title ?? "").length() > 0);
                assert.is_true((truth?.reference_repro ?? "").length() > 0);
            }
        }
    };
}

/// Repro-checker smoke tests that need the runtime compiler.
/// Run with: baml run -e 'smoke()'
function smoke() -> null {
    let fixed = run_repro(
        Repro {
            files: { "main.baml": "function repro_main() -> json { 1 + 1 }" },
            setup: null,
            expectation: ShouldEvaluateTo { check: "should_evaluate_to", expected: 2 },
        },
    );
    assert.equal(fixed.verdict, Verdict.Fixed);

    let broken = run_repro(
        Repro {
            files: { "main.baml": "function repro_main() -> json { 1 + 1 }" },
            setup: null,
            expectation: ShouldEvaluateTo { check: "should_evaluate_to", expected: 3 },
        },
    );
    assert.equal(broken.verdict, Verdict.StillBroken);
    assert.contains(broken.observed, "got 2");

    let rejected = run_repro(
        Repro {
            files: { "main.baml": "function f() -> int { \"not an int\" }" },
            setup: null,
            expectation: ShouldNotCompile { check: "should_not_compile", diagnostic_contains: null },
        },
    );
    assert.equal(rejected.verdict, Verdict.Fixed);

    let compiles = run_repro(
        Repro {
            files: { "main.baml": "function f() -> int { 41 + 1 }" },
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        },
    );
    assert.equal(compiles.verdict, Verdict.Fixed);

    log.info({ "smoke": "ok" });
    null
}
