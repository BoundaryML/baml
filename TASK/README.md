# C# bridge new-run task seed

This folder is the self-contained seed for taking the C#/.NET bridge from the
completed design to a production implementation on current Canary. The earlier
PR #4074 is an experiment and salvage source, not the specification.

## Read order and authority

Read every file in this order before planning or editing implementation code:

1. `bridge-csharp.md` — cross-language parity goal and test discipline.
2. `state-of-python-completeness.md` — reference capability identities and
   current cross-language behavior.
3. `design.md` — **completed normative C# design**. All 20 design questions are
   resolved, subject to the explicitly listed evidence gates.
4. `dry-run-findings.md` — experimental evidence, correctness footguns,
   rejected architecture, and newly exposed reconciliations.
5. `state-of-csharp-completeness.md` — live feature/parity ledger for this run.
6. `verification-gates.md` — live design-evidence, packaging, deployment, and
   release ledger.
7. `AGENT-GOAL.md` — the recommended goal prompt and completion contract.

Authority order:

```text
completed C# design
  > explicit new-run amendments supported by compiled evidence
  > general bridge/parity instructions
  > current implementation details
  > dry-run findings
  > experimental PR behavior
```

Implementation is never allowed to become an accidental design amendment. If a
compiled probe contradicts the design, record the result, amend the design
explicitly, update both ledgers, and only then implement the changed contract.

## Why the raw dry-run folder is not copied here

The experiment's logs are useful, but its design and implementation notes mix
strong evidence with superseded decisions:

- manual `NativeLibrary.GetExport` API-table loading instead of the
  current-target amendment's source-generated `baml_get_api_v1` import plus
  validated typed table;
- reflection-based decoding and no trimming support;
- Base64 bytecode and an invented size limit;
- different callback, stream, media, exception, resource, and output-ownership
  APIs.

Copying those documents wholesale would give an implementation agent two
apparently authoritative designs. `dry-run-findings.md` preserves the measured
results, exact edge cases, reusable fixture ideas, and missing proof while
clearly labeling what must not be transplanted.

The original dry-run folder remains historical provenance outside this seed.
Consult it only to recover exact probe source/commands or to disambiguate a
reported result, never to fill a canonical API gap by precedent.

## Required starting sequence

1. Record the current branch and commit in `verification-gates.md`.
2. Perform the current-Canary semantic integration audit before choosing
   whether to rebase/salvage PR #4074 or implement cleanly from Canary.
3. Compare experimental patches by behavior and ownership, not by file
   survival. Keep only code that matches the completed design and current
   compiler/ABI/release contracts.
4. Resolve gates A1-A8. In particular, do not assume:
   - legacy per-operation exports are the public ABI merely because they are
     visible in one native build; Q1 now follows the canonical
     `baml_get_api_v1` table contract;
   - an arbitrary consumer assembly can access runtime-internal codecs;
   - recursive aliases have a finite erased C# spelling;
   - optional callback arguments can use reflected lambda parameter names;
   - experimental public resource wrappers belong in v1.
5. Complete the evidence required by the design's implementation-document
   entry criteria. Turn the design into a dependency-ordered implementation
   document that names concrete code, tests, packages, and stop conditions.
6. Implement a narrow generated primitive call end to end first. Bring clean
   package consumption forward early enough to expose native/RID/version
   problems rather than leaving packaging to the end.
7. Port shared Python capability tests continuously. Update both ledgers in the
   same change that adds or changes support.
8. Finish the full release path and canonical C# user documentation. “The
   bridge builds” or “local tests pass” is not the completion condition.

## Non-negotiable working rules

- Do not inspect, search, read, summarize, or reference `/engine`.
- Target .NET 10/C# 14. Do not add a net8 target.
- Keep one canonical `baml-bridge` package with the design's RID/package rules.
- Preserve source BAML identity, wire identity, and projected C# names
  separately.
- Keep generated output deterministic, wholly owned, and transactional.
- Do not add `object?`, arbitrary reflection, serializer guessing, silent
  fallback, discovery-order allocation, or allocation-order suffixes to get a
  test green.
- Do not claim `supported` without the exact ledger evidence.
- Do not weaken errors, cancellation, callbacks, streams, media, handles, or
  trimming to match the experiment.
- Do not add a public generated type, helper, environment variable, loader
  path, size ceiling, ABI export, or package target without reconciling it with
  the design and public-surface audit.
- Keep narrowly scoped implementation commits and preserve exact tested
  artifacts through publication.

## Expected durable outputs from the run

- current-target integration audit and implementation document;
- production `baml-bridge` runtime/package and `sdkgen_csharp`;
- `sdk_test_csharp` parity plus C#-specific proof suites;
- completed `state-of-csharp-completeness.md`;
- completed `verification-gates.md` with reproducible evidence;
- exact immutable NuGet build/publish/post-publish path;
- canonical, executable documentation for idiomatic C#-BAML usage.
