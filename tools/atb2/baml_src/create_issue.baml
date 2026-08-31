// feedback -> minimal repro -> already fixed? -> stop | issue (repro + more repros)
//
// Repros are checked with baml.reflect.Package.compile, in-process. No
// subprocess, no VM shellout; the LLM-authored command is never executed.

client Triage = claude_code.ClaudeCodeClient.new(model = "sonnet", timeout_ms = 600000);

// ==================== Pipeline ====================

function create_issue(fb: Feedback) -> Issue? {
    let repro = create_repro(fb);
    let run = run_repro(repro);
    log.info({ "feedback": fb.id, "verdict": run.verdict.to_string(), "observed": run.observed });
    if (run.verdict == Verdict.Fixed) {
        return null;
    };
    let draft = write_issue(fb, repro, run);
    Issue {
        id: "ISSUE-" + baml.id.new(),
        title: draft.title,
        description: draft.description,
        shepherd: null,
        subsystem: draft.subsystem,
        repros: [repro].concat(draft.more_repros),
        version: match (run.verdict) {
            Verdict.StillBroken => toolchain_version(),
            _ => reporter_toolchain(fb),
        },
        feedback_ids: [fb.id],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: draft.resolution_plan,
        difficulty: null,
        design_doc: null,
    }
}

// ==================== 1. Repro (LLM) ====================

// The `check` literal on each Expectation variant is a WIRE discriminator,
// not for code (code matches on the class). Without it the union is
// unparseable: the parser takes the first structurally-matching arm, an
// all-optional class matches any object.
class ShouldCompile {
    check: "should_compile",
}

class ShouldNotCompile {
    check: "should_not_compile",
    /// Substring the diagnostic must contain, if the report names it.
    diagnostic_contains: string?,
}

class ShouldEvaluateTo {
    check: "should_evaluate_to",
    /// The CORRECT value of `repro_main()`.
    expected: json,
}

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
    /// The CORRECT behavior; the bug is fixed when the toolchain matches it.
    expectation: Expectation,
}

function create_repro(fb: Feedback) -> Repro {
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

        Reporter's toolchain: ${reporter_toolchain(fb)}
        Report title: ${fb.title}
        Report body:
        ${fb.body}

        Attached files:
        ${format_files(fb.files)}

        Comment chain:
        ${format_comments(fb.comments)}

        ${ctx.output_format}
    `
}

// ==================== 2. Run repro (in-process) ====================

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
    /// Compiler diagnostics when the sources were rejected.
    error: string?,
    /// Why the check could not run at all (repro needs a human, command is
    /// not verifiable in-process, host has no runtime compiler); null when
    /// the attempt is a real verdict on the sources.
    blocked: string?,
}

function blocked(reason: string) -> CompileAttempt {
    CompileAttempt { pkg: null, error: null, blocked: reason }
}

function try_compile(files: map<string, string>) -> CompileAttempt {
    compile_ok(files) catch (e) {
        baml.reflect.errors.CompilationError => {
            CompileAttempt { pkg: null, error: e.message, blocked: null }
        },
        baml.errors.Unsupported => {
            blocked("runtime compiler unavailable in this host: " + e.message)
        },
        // not a diagnostic about the sources — no verdict can be drawn
        _ => blocked("compilation failed for an unexpected reason"),
    }
}

function compile_ok(files: map<string, string>) -> CompileAttempt throws unknown {
    let pkg = baml.reflect.Package.compile(files);
    CompileAttempt { pkg: pkg, error: null, blocked: null }
}

/// The in-process check compiles (and, for should_evaluate_to, evaluates)
/// the repro's files. That covers `baml check` / `baml run` / `baml build`
/// surfaces; `baml generate`, `baml test`, `baml fmt`, `baml pack` ... can
/// fail even when the sources compile, so they are never auto-verdicted.
function command_covered_by_compile(command: string) -> bool {
    let c = command.trim();
    c.starts_with("baml check") || c.starts_with("baml run") || c.starts_with("baml build")
}

/// Whether this host can compile in-process. The `baml test` VM cannot;
/// there every check comes back blocked.
function runtime_compiler_available() -> bool {
    try_compile({ "probe.baml": "function probe() -> int { 1 }" }).blocked == null
}

/// Compile the repro, unless something rules the check out first.
function attempt_repro(repro: Repro) -> CompileAttempt {
    match (repro.expectation) {
        let e: RequiresInspection => blocked("requires inspection: " + e.instructions),
        _ => {
            if (command_covered_by_compile(repro.command)) {
                try_compile(repro.files)
            } else {
                blocked(
                    "command-dependent repro ("
                        + repro.command.trim()
                        + ") is not verifiable by an in-process compile; requires inspection",
                )
            }
        },
    }
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
    let attempt = attempt_repro(repro);
    if (attempt.blocked != null) {
        return ReproRun { verdict: Verdict.Inconclusive, observed: attempt.blocked ?? "" };
    };
    match (repro.expectation) {
        let e: ShouldCompile => match (attempt.error) {
            null => ReproRun { verdict: Verdict.Fixed, observed: "compiles cleanly" },
            let msg: string => {
                ReproRun { verdict: Verdict.StillBroken, observed: "still rejected: " + msg }
            },
        },
        let e: ShouldNotCompile => match (attempt.error) {
            null => {
                ReproRun {
                    verdict: Verdict.StillBroken,
                    observed: "still accepted (compiles cleanly)",
                }
            },
            let msg: string => {
                let wanted = e.diagnostic_contains;
                if (wanted != null && !msg.includes(wanted ?? "")) {
                    ReproRun {
                        verdict: Verdict.Inconclusive,
                        observed: "rejected, but diagnostic differs: " + msg,
                    }
                } else {
                    ReproRun { verdict: Verdict.Fixed, observed: "now rejected: " + msg }
                }
            },
        },
        let e: ShouldEvaluateTo => match (attempt.pkg) {
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
        },
        // handled by attempt_repro (blocked above)
        let e: RequiresInspection => {
            ReproRun { verdict: Verdict.Inconclusive, observed: e.instructions }
        },
    }
}

// ==================== 3. Issue (LLM) ====================

/// The LLM-authored parts of a ticket; id, status, version and the minimal
/// repro are filled in by create_issue.
class IssueDraft {
    title: string,
    description: string,
    subsystem: Subsystem,
    resolution_plan: string?,
    /// 1-2 further repros exercising the same defect from a different angle
    /// (another surface, a variant trigger); the minimal repro is added
    /// separately.
    more_repros: Repro[],
}

function write_issue(fb: Feedback, repro: Repro, run: ReproRun) -> IssueDraft {
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
          languages (sdkgen/codegen), IDE/playground, packaging, CI;
          unknown = none of these fits or the defect spans several.
        - resolution_plan: a short outline of a plausible fix (files/phases to
          look at, suggested approach), or null if you cannot say anything
          non-obvious.
        - more_repros: 1-2 additional repros for the SAME defect that a
          maintainer would want alongside the minimal one — a variant trigger
          or another command surface. Same shape as the minimal repro
          (files + command + expectation); empty if nothing meaningful adds.

        Original report (toolchain ${reporter_toolchain(fb)}): ${fb.title}
        ${fb.body}

        Minimal repro command: ${repro.command}

        Minimal repro files:
        ${format_files(repro.files)}

        Checker verdict on the current toolchain: ${run.verdict.to_string()}
        Observed: ${run.observed}

        ${ctx.output_format}
    `
}

// ==================== Helpers ====================

function toolchain_version() -> string {
    baml.env.get("BAML_VERSION") ?? "unknown"
}

function format_comments(comments: Comment[]) -> string {
    comments
        .map((c: Comment) -> string {
            c.author + ": " + c.body
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

// ==================== Eval ====================
//
// Two metrics, each a testset with testing.PassRate(0.8):
//
//   repro_match      from feedback, does create_repro produce the repro the
//                    issue calls for? Keyword recall against the reference
//                    repro's input tokens (>= 0.7), and — when the toolchain
//                    can run it — the repro must actually compile/evaluate.
//   issue_enrichment given feedback and the reference repro, does write_issue
//                    produce the issue a maintainer wants? LLM-judged
//                    similarity against the reference issue (>= 0.7).
//   difficulty_estimate given the reference issue, does estimate_difficulty
//                    land on the maintainer's difficulty? Exact match, one
//                    test per issue.
//
// The dataset lives in Supabase (triage_issues / triage_feedback, see
// eval/supabase/schema.sql): feedback is what agent sessions passed to
// `baml feedback`; reference repros come from the issue when it has one
// (repro_source = github) and are LLM-authored otherwise (generated).
// FEEDBACK_SUPABASE_URL + FEEDBACK_SUPABASE_KEY (anon, read-only) select it; without them the
// testsets have no tests.
//   baml test -i "root::repro_match::*" -i "root::issue_enrichment::*" -i "root::difficulty_estimate::*"

/// The issue as the pipeline should write it: IssueDraft's shape with the
/// full set of good repros, minimal first.
class ReferenceIssue {
    title: string,
    description: string,
    subsystem: Subsystem,
    resolution_plan: string?,
    repros: Repro[],
    /// How much work the fix is, as a maintainer would gauge it.
    difficulty: Difficulty,
}

/// A row of triage_issues.
class IssueRow {
    number: int,
    title: string,
    /// The reference repro (verbatim from the issue, or authored).
    reference_repro: string,
    /// "github" = the issue's own Reproduction section; "generated" = LLM-authored.
    repro_source: string,
    /// Input tokens a repro must contain to exercise the same defect.
    keywords: string[],
    reference_issue: ReferenceIssue,
}

/// A row of triage_feedback — a Feedback with its author flattened.
class FeedbackRow {
    id: string,
    issue_number: int,
    title: string,
    body: string,
    source: FeedbackSource,
    author_email: string?,
    author_github: string?,
    author_device_id: string?,
    toolchain: string,
    files: map<string, string>,
    session: string?,
}

function to_feedback(r: FeedbackRow) -> Feedback {
    Feedback {
        id: r.id,
        title: r.title,
        body: r.body,
        source: r.source,
        author: Author { email: r.author_email, github: r.author_github, device_id: r.author_device_id },
        toolchain: r.toolchain,
        files: r.files,
        comments: [],
        issue_ids: [],
    }
}

function supabase_configured() -> bool {
    baml.env.get("FEEDBACK_SUPABASE_URL") != null && baml.env.get("FEEDBACK_SUPABASE_KEY") != null
}

/// GET a PostgREST query and return the JSON body.
function supabase_get(query: string) -> string {
    let url = (baml.env.get("FEEDBACK_SUPABASE_URL") ?? "") + "/rest/v1/" + query;
    let key = baml.env.get("FEEDBACK_SUPABASE_KEY") ?? "";
    let res = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: url,
            headers: { "apikey": key, "Authorization": "Bearer " + key, "Accept": "application/json" },
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (e) {
        _ => baml.sys.panic("supabase: request failed for " + url),
    };
    if (res.status_code != 200) {
        baml.sys.panic("supabase: " + res.status_code.to_string() + " for " + url);
    };
    res.text() catch (_) {
        _ => baml.sys.panic("supabase: unreadable body for " + url),
    }
}

function load_issues() -> map<string, IssueRow> {
    let out: map<string, IssueRow> = {};
    for (
        let row in baml.json.from_string<IssueRow[]>(
            supabase_get("triage_issues?select=*&order=number"),
        )
    ) {
        out.set(row.number.to_string(), row);
    }
    out
}

function load_feedback() -> FeedbackRow[] {
    baml.json.from_string<FeedbackRow[]>(supabase_get("triage_feedback?select=*&order=id"))
}

/// Fraction of `keywords` present in `ours` (case-insensitive; "baml-cli"
/// and "baml" name the same binary). 0.0 when there are no keywords.
function keyword_recall(ours: string, keywords: string[]) -> float {
    let hay = ours.to_lower_case().replace("baml-cli ", "baml ");
    let hit = keywords.filter((k: string) -> bool {
        hay.includes(k.to_lower_case().replace("baml-cli ", "baml "))
    });
    if (keywords.length() == 0) {
        0.0
    } else {
        (1.0 * hit.length()) / (1.0 * keywords.length())
    }
}

function draft_text(d: IssueDraft) -> string {
    baml.json.stringify_pretty(d.to_json()) catch (_) {
        _ => "<unserializable>",
    }
}

function reference_text(r: ReferenceIssue) -> string {
    baml.json.stringify_pretty(r.to_json()) catch (_) {
        _ => "<unserializable>",
    }
}

class IssueJudgment {
    /// 0.0 (different defect / unusable ticket) .. 1.0 (a maintainer would
    /// accept OURS in place of the REFERENCE).
    similarity: float,
    /// What OURS gets wrong or misses relative to the reference.
    missing: string?,
}

/// Judge our ticket against the reference ticket for the same issue.
function judge_issue(ours: string, reference: string) -> IssueJudgment {
    client: Triage
    prompt: `
        Two engineering tickets for the same BAML toolchain bug. REFERENCE is
        how the maintainers want it written; OURS was produced automatically
        from a user report. Score how well OURS stands in for REFERENCE:
        same defect and subsystem, description conveys the same symptoms and
        expected-vs-actual, repros exercise the same trigger, resolution plan
        points the same way. Wording and length do not matter.
        1.0 = interchangeable; 0.7 = same defect, minor gaps; 0.4 = related
        but a maintainer would have to rewrite it; 0.0 = wrong defect.

        REFERENCE:
        ${reference}

        OURS:
        ${ours}

        ${ctx.output_format}
    `
}

testset "repro_match" with testing.PassRate(0.8) {
    if (supabase_configured()) {
        let issues = load_issues();
        for (let row in load_feedback()) {
            test (row.id) {
                // `let`s in the loop body reach these closures with the LAST
                // iteration's value, so everything is derived from `row`.
                let key = row.issue_number.to_string();
                let issue = issues.get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                let repro = create_repro(to_feedback(row));
                let recall = keyword_recall(format_repro(repro), issue.keywords);
                let run = run_repro(repro);
                log.info(
                    {
                        "feedback": row.id,
                        "issue": key,
                        "repro_source": issue.repro_source,
                        "keyword_recall": recall,
                        "command": repro.command,
                        "run": run.verdict.to_string(),
                        "observed": run.observed,
                    },
                );
                assert.is_true(recall >= 0.7);
                // a programmatic repro must actually run when this host can
                // run it (the `baml test` VM has no runtime compiler — then
                // only recall is asserted)
                let programmatic = match (repro.expectation) {
                    let e: RequiresInspection => false,
                    _ => command_covered_by_compile(repro.command),
                };
                if (programmatic && runtime_compiler_available()) {
                    assert.is_true(run.verdict != Verdict.Inconclusive);
                };
            }
        }
    };
}

testset "issue_enrichment" with testing.PassRate(0.8) {
    if (supabase_configured()) {
        let issues = load_issues();
        for (let row in load_feedback()) {
            test (row.id) {
                let key = row.issue_number.to_string();
                let issue = issues.get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                let reference = issue.reference_issue;
                let repro = reference.repros.at(0)
                    ?? baml.sys.panic("reference issue #" + key + " has no repros");
                let draft = write_issue(
                    to_feedback(row),
                    repro,
                    ReproRun {
                        verdict: Verdict.Inconclusive,
                        observed: "not run during enrichment eval",
                    },
                );
                let j = judge_issue(draft_text(draft), reference_text(reference));
                log.info(
                    {
                        "feedback": row.id,
                        "issue": key,
                        "similarity": j.similarity,
                        "missing": j.missing,
                    },
                );
                assert.is_true(j.similarity >= 0.7);
            }
        }
    };
}

/// The reference issue as an Issue, so pipeline stages can be run on it.
function reference_as_issue(number: int, r: ReferenceIssue) -> Issue {
    Issue {
        id: "GH-" + number.to_string(),
        title: r.title,
        description: r.description,
        shepherd: null,
        subsystem: r.subsystem,
        repros: r.repros,
        version: "unknown",
        feedback_ids: [],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: r.resolution_plan,
        difficulty: null,
        design_doc: null,
    }
}

testset "difficulty_estimate" with testing.PassRate(0.8) {
    if (supabase_configured()) {
        let issues = load_issues();
        for (let key in issues.keys()) {
            test ("#" + key) {
                let issue = issues.get(key) ?? baml.sys.panic("no triage_issues row for #" + key);
                let estimate = estimate_difficulty(reference_as_issue(issue.number, issue.reference_issue));
                log.info(
                    {
                        "issue": key,
                        "expected": issue.reference_issue.difficulty.to_string(),
                        "got": estimate.difficulty.to_string(),
                        "rationale": estimate.rationale,
                    },
                );
                assert.equal(estimate.difficulty, issue.reference_issue.difficulty);
            }
        }
    };
}

// ==================== Unit tests ====================
// Token-free: parsing runs through the same machinery as live replies
// (Fn$parse / from_string). Checks needing the runtime compiler (absent in
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
              "toolchain": "0.17.0",
              "files": {},
              "comments": [],
              "issue_ids": []
            }
        "#;
        let fb = baml.json.from_string<Feedback>(raw);
        assert.equal(author_identity(fb.author), "octocat");
    }

    test "a report without a toolchain still parses and reads as unknown" {
        let raw = #"
            {
              "id": "FB-3",
              "title": "t",
              "body": "b",
              "source": "BamlFeedback",
              "author": { "email": null, "github": null, "device_id": "dev-1" },
              "toolchain": null,
              "files": {},
              "comments": [],
              "issue_ids": []
            }
        "#;
        let fb = baml.json.from_string<Feedback>(raw);
        assert.equal(reporter_toolchain(fb), "unknown");
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
    test "command-dependent repro is never auto-verdicted" {
        let r = Repro {
            files: { "main.baml": "function f() -> int { 42 }" },
            command: "baml generate",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        };
        let run = run_repro(r);
        assert.equal(run.verdict, Verdict.Inconclusive);
        assert.contains(run.observed, "command-dependent");
    }

    test "requires_inspection is inconclusive" {
        let r = Repro {
            files: {},
            command: "baml fmt main.baml",
            setup: null,
            expectation: RequiresInspection { check: "requires_inspection", instructions: "look at fmt output" },
        };
        assert.equal(run_repro(r).verdict, Verdict.Inconclusive);
    }

    test "a compiler-less host degrades to inconclusive; a host with one verdicts" {
        let r = Repro {
            files: { "main.baml": "function f() -> int { 41 + 1 }" },
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        };
        let run = run_repro(r);
        if (runtime_compiler_available()) {
            assert.equal(run.verdict, Verdict.Fixed);
        } else {
            assert.equal(run.verdict, Verdict.Inconclusive);
        }
    }
}

testset "llm_parsing" {
    test "create_repro reply parses into a typed Repro" {
        let raw = #"
            {
              "files": { "main.baml": "function repro_main() -> json { [1,2].at(5) ?? -1 }" },
              "command": "baml run -e repro_main()",
              "setup": null,
              "expectation": { "check": "should_evaluate_to", "expected": -1 }
            }
        "#;
        let repro = create_repro$parse(raw);
        assert.is_true(repro.files.has("main.baml"));
        match (repro.expectation) {
            let e: ShouldEvaluateTo => assert.equal(baml.json.stringify(e.expected), "-1"),
            _ => baml.sys.panic("expected should_evaluate_to"),
        }
    }

    test "write_issue reply parses with more repros" {
        let raw = #"
            {
              "title": "Array.at returns wrong element",
              "description": "…",
              "subsystem": "StdLibrary",
              "resolution_plan": "audit bounds check in array intrinsics",
              "more_repros": [
                {
                  "files": { "main.baml": "function repro_main() -> json { [1].at(-1) ?? -1 }" },
                  "command": "baml run -e repro_main()",
                  "setup": null,
                  "expectation": { "check": "should_evaluate_to", "expected": -1 }
                }
              ]
            }
        "#;
        let draft = write_issue$parse(raw);
        assert.equal(draft.subsystem, Subsystem.StdLibrary);
        assert.equal(draft.more_repros.length(), 1);
    }
}

testset "judge_parsing" {
    test "judge_issue reply parses" {
        let j = judge_issue$parse(#"{ "similarity": 0.8, "missing": null }"#);
        assert.equal(j.similarity, 0.8);
    }
}

testset "keyword_recall" {
    test "case-insensitive, cli spelling ignored" {
        let r = keyword_recall(
            "$ baml generate\n\n--- baml.toml ---\noutput_type = \"rust\"",
            ["baml-cli generate", "output_type = \"rust\"", "throws", "-F beta"],
        );
        assert.equal(r, 0.5);
    }

    test "no keywords is zero" {
        assert.equal(keyword_recall("anything", []), 0.0);
    }
}

/// Checker smoke tests that need the runtime compiler.
/// Run with: baml run -e 'smoke()'
function smoke() -> null {
    let fixed = run_repro(
        Repro {
            files: { "main.baml": "function repro_main() -> json { 1 + 1 }" },
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldEvaluateTo { check: "should_evaluate_to", expected: 2 },
        },
    );
    assert.equal(fixed.verdict, Verdict.Fixed);

    let broken = run_repro(
        Repro {
            files: { "main.baml": "function repro_main() -> json { 1 + 1 }" },
            command: "baml run main.baml",
            setup: null,
            expectation: ShouldEvaluateTo { check: "should_evaluate_to", expected: 3 },
        },
    );
    assert.equal(broken.verdict, Verdict.StillBroken);
    assert.contains(broken.observed, "got 2");

    let rejected = run_repro(
        Repro {
            files: { "main.baml": "function f() -> int { \"not an int\" }" },
            command: "baml check",
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
