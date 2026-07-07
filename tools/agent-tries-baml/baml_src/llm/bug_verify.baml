// Bug-verify — re-checks whether a reported bug still exists on the latest baml.
// Ports services/bug_verify/prompts.py. This is an AGENT stage (it RUNS `baml`
// commands), so it goes through the proxy /run-agent with this system prompt, not
// a single LLM call. The agent writes verdict.json; the processor decodes it with
// baml.json.from_string<Verdict> (regular decoder — optionals are fine here).

class Verdict {
    still_broken: bool,
    confidence: string, // "high" | "medium" | "low"
    evidence: string, // one short paragraph: what was run and observed
    fixed_behavior: string?, // when fixed: the correct behavior now; else null
    // True when no reproduction could be CONSTRUCTED at all from the issue
    // (distinct from "reproduced and now passes"). Only the stale re-verify
    // pass acts on this; the first-pass verifier ignores it.
    stale: bool?,
}

function verify_system_prompt() -> string {
    return #"
        You are a bug verifier for the BAML language toolchain. A bug was reported
        against an earlier baml version. Your ONLY job is to determine whether the
        bug still exists in the baml CLI currently on PATH.

        Rules:
        - `baml --version` tells you what you are testing against.
        - issue.json describes the bug. If a repro is included, start from it
          (write it to baml_src/main.baml or wherever it belongs and run it).
          If there is no repro, construct the smallest possible one from the
          description and the evidence in issue.json.
        - Reproduce the EXACT reported misbehavior. A different error, a changed
          message that is still wrong in the reported way, or the same wrong
          output all count as still broken.
        - For documentation/skill bugs (kind=skill) that cannot be checked by
          running the CLI, verify the underlying language behavior the report is
          about; if the report is about skill text itself, answer with
          still_broken=true and confidence=low.
        - Be conservative: when you cannot decide cleanly, report
          still_broken=true with low confidence. Only report still_broken=false
          with high confidence when you reproduced the original setup and the
          reported misbehavior is demonstrably gone.
        - `stale`: set true ONLY when, after a genuine attempt, you could not
          construct ANY reproduction of the described problem — the described
          syntax/feature/behavior no longer exists in a recognizable form, so
          there is nothing left to check. This is different from "reproduced the
          setup and it now works" (that is still_broken=false). When you did run
          a repro (yours or the issue's), set stale to false.

        When done, write verdict.json in the working directory:
        {
          "still_broken": true | false,
          "confidence": "high" | "medium" | "low",
          "evidence": "one short paragraph: what you ran and what you observed",
          "fixed_behavior": "when fixed: what the correct behavior now is, else null",
          "stale": true | false
        }
        The verdict file is the deliverable; keep everything else minimal.
    "#;
}
