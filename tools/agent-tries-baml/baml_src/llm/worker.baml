// Worker agent — does the BAML task AND writes its own verbose trophy.json
// self-report. Ports services/baml_worker/prompts.py. AGENT stage: runs Claude
// Code via the proxy with one of these system prompts; the processor decodes the
// agent's trophy.json with baml.json.from_string<TrophyReport>.
//
// Two headers share one TROPHY_SPEC: warm start (baml on PATH, skills installed)
// and cold start (install BAML from the quickstart — onboarding is under test).

class TrophyRepro {
    files: map<string, string>, // smallest baml that triggers it (no baml.toml/generator)
    command: string, // exact baml command, e.g. "baml generate"
    should_fail: bool, // true if the command should error; false if it should misbehave
}

class TrophyIssue {
    kind: string, // "skill" | "language"
    title: string, // <= 80 chars
    description: string,
    call_index: int?, // 1-based turn where it surfaced, or null
    suggestion: string,
    repro: TrophyRepro?, // REQUIRED for every kind="language" issue
}

class TrophySuggestion {
    target: string, // "skill" | "language"
    suggestion: string,
    rationale: string,
}

class TrophyReport {
    report_md: string,
    task_completed: string, // "true" | "false" | "partial"
    summary: string,
    what_went_well: string[],
    what_failed: string[],
    issues: TrophyIssue[],
    suggestions: TrophySuggestion[],
}

// The trophy.json reporting spec, shared by both modes.
function trophy_spec() -> string {
    return #"
        When you are DONE with the task, write a single file `trophy.json` in the working directory: your own report of this run. Be exhaustive and verbose. This report is the only record of what happened and is read by a downstream agent, so richer is better.

        trophy.json shape:
        {
          "report_md": "<full markdown narrative: ## What worked, ## What didn't work / bugs, ## Stdlib or doc gaps, ## Suggestions for BAML team>",
          "task_completed": "true" | "false" | "partial",
          "summary": "<2-4 sentences: did you deliver working BAML, and how much friction did you hit?>",
          "what_went_well": ["short bullets"],
          "what_failed": ["short bullets"],
          "issues": [
            { "kind": "skill" | "language", "title": "<= 80 chars",
              "description": "<verbose: what happened, the exact error/message text, why it is a problem>",
              "call_index": <1-based turn number where it surfaced, or null; do not guess>,
              "suggestion": "<definitive fix: exactly what should change in the skill docs or the language/CLI>",
              "repro": { "files": { "baml_src/main.baml": "<smallest baml that triggers it>" }, "command": "baml generate", "should_fail": true } }
          ],
          "suggestions": [
            { "target": "skill" | "language", "suggestion": "<concrete improvement>", "rationale": "<why it matters>" }
          ]
        }

        Catch ALL of it. Report every baml error, every non-zero `baml` command, every confusing or unhelpful compiler/CLI message (even when the command exited 0), every stdlib or doc gap, every workaround you had to invent, EVEN IF you recovered and the final result works. Succeeding does NOT mean `issues` is empty.

        issue kinds:
        - "skill": the BAML skill docs (installed skills / SKILL.md) are wrong, missing, or unclear.
        - "language": the BAML compiler / CLI / language itself has a bug or limitation.
        - If fixing the language would obviate the skill change, it is a language issue.

        repro rules: EVERY `kind="language"` issue MUST carry a `repro`. Before you report it, actually RUN the exact `command` on that minimal `baml_src` yourself and confirm it produces the problem. Give the SMALLEST `baml_src/*.baml` that triggers the issue. A baml.toml and a generator block are auto-provided by the verifier, so do NOT include them. "should_fail" is true if the command should error, false if it should succeed but misbehaves. Omit "repro" ONLY for a pure skill/docs-wording gap that no baml command can demonstrate.

        Write trophy.json LAST, after the task is complete, and write ONLY valid JSON in it.
    "#;
}

// Warm start: prebuilt baml on PATH; the agent installs the official skills
// itself — the install step is part of the benchmarked behavior.
function worker_system_prompt() -> string {
    return #"
        You are an engineer writing BAML to complete the task below. FIRST, run `baml agent install --latest` — it installs the official BAML agent skills into this project under .claude/skills/ (namespaced baml-*). Then read the relevant installed skills before you start writing code. The `baml` CLI is on PATH; use it to compile, run, format, and test your work.
    "#
        + trophy_spec();
}

// Cold start: nothing installed, no skill guide — the agent installs BAML from
// the quickstart itself; onboarding friction is a first-class finding.
function cold_start_system_prompt(quickstart_url: string) -> string {
    return #"
        You are an engineer who must complete the BAML task below -- but BAML is NOT installed and NO skill guide is provided. Before you can do anything, set BAML up yourself:

        1. Open
    "#
        + quickstart_url
        + #"
            and read it.
            2. Download the install script it points you to and run it to install the `baml` CLI.
            3. Confirm the install worked (`baml --version`) before starting the task.

            Then use `baml` to compile, run, format, and test your work as normal.

            This is a cold-start run: the onboarding experience IS under test. Treat every install/quickstart/doc-discovery problem as a first-class finding -- anything confusing, missing, wrong, or that made you guess. Report install/CLI/setup bugs and doc/quickstart gaps in `issues` exactly like language and skill issues (`kind="skill"` for docs/quickstart wording or missing steps, `kind="language"` for CLI/installer/compiler bugs).
        "#
        + trophy_spec();
}
