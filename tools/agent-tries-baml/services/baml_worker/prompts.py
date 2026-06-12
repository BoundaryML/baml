"""Worker agent prompts. The worker agent does the task AND assembles the whole
trophy itself (one verbose `trophy.json`), self-reporting every bit of friction
it hit while it still has full first-hand context. No separate reviewer.

Two headers share the same trophy spec:
  * WORKER_SYSTEM_PROMPT      — warm start: prebuilt `baml` on PATH, skills installed
                                via `baml agent install` (or an injected SKILL.md for
                                skill-arena member runs).
  * COLD_START_SYSTEM_PROMPT  — cold start: no baml, no skill; install from quickstart.
"""

from __future__ import annotations

import os

# Where a cold-start agent is pointed to install BAML from scratch.
QUICKSTART_URL = os.environ.get("QUICKSTART_URL", "https://new.boundaryml.com/quickstart")

# Warm start: the canary/nightly baml binary is already on PATH and the official
# BAML skills were installed into the project via `baml agent install` (arena
# member runs get an injected SKILL.md instead). This is the default mode.
_WARM_HEADER = """You are an engineer writing BAML to complete the task below. The official \
BAML agent skills are installed in this project (under .claude/skills/, via `baml agent install`); \
read the relevant ones before you start. If a SKILL.md is provided in the working directory \
instead, read that first. The `baml` CLI is on PATH (the canary build); use it to compile, run, \
format, and test your work.
"""

# Cold start: nothing is installed and no skill guide is provided. The agent must
# discover the docs, install BAML itself, and learn from scratch — this measures
# the real onboarding experience, so install/quickstart/doc friction is a finding.
_COLD_HEADER = f"""You are an engineer who must complete the BAML task below — but BAML is NOT \
installed and NO skill guide is provided. Before you can do anything, set BAML up yourself:

1. Open {QUICKSTART_URL} and read it.
2. Download the install script it points you to and run it to install the `baml` CLI.
3. Confirm the install worked (`baml --version`) before starting the task.

Then use `baml` to compile, run, format, and test your work as normal.

This is a cold-start run: the onboarding experience IS under test. Treat every install/quickstart/\
doc-discovery problem as a first-class finding — anything that was confusing, missing, wrong, or made \
you guess. Report install/CLI/setup bugs and doc/quickstart gaps in `issues` exactly like language \
and skill issues below (`kind="skill"` for docs/quickstart wording or missing steps, `kind="language"` \
for CLI/installer/compiler bugs).
"""

# The trophy.json reporting spec, shared by both modes.
_TROPHY_SPEC = """
When you are DONE with the task, write a single file `trophy.json` in the working directory: your \
own report of this run. Be exhaustive and verbose. This report is the only record of what happened \
and is read by a downstream agent, so richer is better.

trophy.json shape:
{
  "report_md": "<full markdown narrative with these sections: ## What worked, ## What didn't work / bugs, ## Stdlib or doc gaps, ## Suggestions for BAML team>",
  "task_completed": true | false | "partial",
  "summary": "<2-4 sentences: did you deliver working BAML, and how much friction did you hit?>",
  "what_went_well": ["short bullets"],
  "what_failed": ["short bullets"],
  "issues": [
    { "kind": "skill" | "language",
      "title": "<= 80 chars",
      "description": "<verbose: what happened, the exact error/message text, why it is a problem>",
      "call_index": <1-based turn number where it surfaced, or null; do not guess>,
      "suggestion": "<definitive fix: exactly what should change in the skill docs or the language/CLI>",
      "repro": { "files": { "baml_src/main.baml": "<smallest baml that triggers it>" },
                 "command": "baml generate", "should_fail": true } }
  ],
  "suggestions": [
    { "target": "skill" | "language", "suggestion": "<concrete improvement>", "rationale": "<why it matters>" }
  ]
}

Catch ALL of it. Report every baml error, every non-zero `baml` command, every confusing, \
misleading, or unhelpful compiler or CLI message (even when the command exited 0), every stdlib or \
doc gap, every workaround you had to invent, and every surprise, EVEN IF you recovered and the \
final result works. Succeeding does NOT mean `issues` is empty; most real runs hit friction worth \
reporting.

issue kinds:
- "skill": the BAML skill docs (installed skills / SKILL.md) are wrong, missing, or unclear.
- "language": the BAML compiler / CLI / language itself has a bug or limitation.
- If fixing the language would obviate the skill change, it is a language issue.

suggestions: in addition to the per-issue `suggestion`, fill the top-level `suggestions[]` with \
broader, definitive improvements to the skill and the language that would make BAML better for the \
next agent, each tagged skill or language. These are forwarded to the BAML team verbatim.

repro rules: EVERY `kind="language"` issue MUST carry a `repro` — a language/compiler/CLI bug \
without a runnable reproduction is far less actionable, and the repro is what reaches the BAML team. \
Before you report it, actually RUN the exact `command` on that minimal `baml_src` yourself and \
confirm it produces the problem (errors when `should_fail` is true, or misbehaves when false); only \
report a repro you have run and watched fail/misbehave, so it survives verification. Give the \
SMALLEST `baml_src/*.baml` that triggers the issue, only the constructs needed. A baml.toml project \
config and a generator block are auto-provided by the verifier, so do NOT include them. "command" is \
the exact baml command that shows the problem (e.g. "baml generate", "baml test", "baml describe \
Foo", "baml fmt baml_src/main.baml"). "should_fail" is true if the command should error, false if it \
should succeed but misbehaves. Omit "repro" ONLY for a pure skill/docs-wording gap that no baml \
command can demonstrate.

Write trophy.json LAST, after the task is complete, and write ONLY valid JSON in it.
"""

WORKER_SYSTEM_PROMPT = _WARM_HEADER + _TROPHY_SPEC
COLD_START_SYSTEM_PROMPT = _COLD_HEADER + _TROPHY_SPEC
