// feedback -> minimal repro -> already fixed? -> stop | issue (repro + more repros)
//
// Repros are checked with baml.reflect.Package.compile, in-process. No
// subprocess, no VM shellout; the LLM-authored command is never executed.

// The locally-authenticated Claude Code CLI — no API key needed.
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
        version: toolchain_version(),
        feedback_ids: [fb.id],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: draft.resolution_plan,
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

        Reporter's toolchain: ${fb.toolchain}
        Report title: ${fb.title}
        Report body:
        ${fb.body}

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
        _ => {
            CompileAttempt {
                pkg: null,
                error: "compilation failed for an unexpected reason",
                blocked: null,
            }
        },
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
          languages (sdkgen/codegen), IDE/playground, packaging, CI.
        - resolution_plan: a short outline of a plausible fix (files/phases to
          look at, suggested approach), or null if you cannot say anything
          non-obvious.
        - more_repros: 1-2 additional repros for the SAME defect that a
          maintainer would want alongside the minimal one — a variant trigger
          or another command surface. Same shape as the minimal repro
          (files + command + expectation); empty if nothing meaningful adds.

        Original report (toolchain ${fb.toolchain}): ${fb.title}
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
// THE metric: from feedback that could have led to a GitHub issue, does
// create_repro produce the repro the maintainer put in that issue? Scored as
// keyword recall: the issue's reference repro is reduced (once, at dataset
// build time) to the input tokens a repro must contain — subcommand, flags,
// constructs, option names — and ours must contain >= 70% of them.
//
// The dataset (eval/, gitignored) is built by the gitignored generator:
//   baml run -e 'build_eval_dataset(60, 3)'
// The metric is the testset below:
//   baml test -i "repro_match::*"

class IssueTruth {
    title: string,
    /// The maintainer's own repro from the GitHub issue (verbatim).
    reference_repro: string,
    /// Input tokens a repro must contain to exercise the same defect.
    keywords: string[],
}

class GroundTruth {
    issues: map<string, IssueTruth>,
    feedback_to_issue: map<string, int>,
    duplicate_groups: string[][],
}

function load_ground_truth(dir: string = "eval") -> GroundTruth {
    baml.json.from_string<GroundTruth>(baml.fs.read(dir + "/ground_truth.json"))
}

function load_feedback(dir: string, fid: string) -> Feedback {
    baml.json.from_string<Feedback>(baml.fs.read(dir + "/feedback/" + fid + ".json"))
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

// One test per feedback record; the testset passes when >= 25% of repros
// match their issue's reference. Live LLM calls — only exists when the
// dataset is present locally.
testset "repro_match" with testing.PassRate(0.25) {
    if (baml.fs.exists("eval/ground_truth.json")) {
        let gt = load_ground_truth();
        for (let fid in gt.feedback_to_issue.keys()) {
            test (fid) {
                // `let`s in the loop body reach these closures with the LAST
                // iteration's value (only the loop variable is captured per
                // iteration), so everything is derived from `fid` in here.
                let key = (gt.feedback_to_issue.get(fid) ?? 0).to_string();
                let truth = gt.issues.get(key) ?? baml.sys.panic("no ground truth for issue " + key);
                let repro = create_repro(load_feedback("eval", fid));
                let recall = keyword_recall(format_repro(repro), truth.keywords);
                log.info(
                    {
                        "feedback": fid,
                        "issue": key,
                        "keyword_recall": recall,
                        "command": repro.command,
                    },
                );
                assert.is_true(recall >= 0.7);
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
