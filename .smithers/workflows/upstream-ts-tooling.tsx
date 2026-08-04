// smithers-display-name: Upstream TS Tooling
// smithers-source: one-off — bring a first-class TypeScript tooling stack (compiler bridge,
// unplugin, TS language-service features incl. go-to-definition) to BAML and open a DRAFT
// PR against BoundaryML/baml from the user's fork. The final task removes every piece of
// Smithers scaffolding this effort introduced, including this file, as the PR's last commit.
//
// Run with the WORKING-TREE Smithers CLI (never a published build), from the baml repo root:
//   bun /Users/williamcory/smithers/apps/cli/src/index.js up .smithers/workflows/upstream-ts-tooling.tsx -d
// Then keep a supervisor attached for the whole run (the first execution lost ~8h to a
// dead engine sitting in a stale quota park; supervision is not optional):
//   bun /Users/williamcory/smithers/apps/cli/src/index.js supervise -r <run-id> --interval 30s --stale-threshold 5m
//
// ── Layout ─────────────────────────────────────────────────────────────────────
// This file is the thin composition only: input resolution, legacy/fresh
// detection, ordering, and conditionals. Everything else lives beside it:
//   ../tasks/*.tsx    one file per task node (its zod output schema + <Task> component)
//   ../prompts/*.mdx  one MDX file per agent prompt (prose; dynamic pieces are props)
//   ../lib/*.ts       shared constants, helpers, agents, prompt fragments, plan readers
//
// ── Retrospective changes (2026-08-04), from the first full execution ──────────
// Each change below exists because the first run paid for its absence:
// 1. TECHNIQUES-NOT-STRUCTURE: the original prompts said "TEVM-grade" and led with a
//    TEVM architecture artifact, which anchored every design on TEVM's PACKAGE LAYOUT.
//    The user had to steer mid-flight and a whole consolidation refactor round was
//    spent merging packages. Structure rules (follow the target repo's conventions,
//    fewest packages, justified diff) are now in the brief, the research artifact
//    contract, and every design/synthesis/implement/review prompt from round zero.
// 2. CLEAN-CONSUMER PROOF: the worst defect found in review (packed native bridge could
//    never load — bare specifier mangled into a filesystem path) survived implement
//    because every test set the internal BAML_TOOLING_BRIDGE_PATH env hook. A packed
//    fresh-consumer e2e with ZERO internal env vars is now a hard deliverable and a
//    blocking review criterion.
// 3. DECLARATION/RUNTIME PARITY: enums were declared in the .d.ts but absent from the
//    emitted ESM (one generator, two projections). Parity tests (runtime module exports
//    exactly what the declarations claim) are now a hard deliverable.
// 4. MILESTONE IMPLEMENTATION: the single 8-hour implement node was the biggest
//    operational liability — stream disconnects, a graceful pause that ate the final
//    JSON report (work fine, report lost, guard failed), and wall-clock burned on
//    re-validation after each resume. Fresh runs now implement the synthesized
//    atomic commit plan as parallel worktree LANES — one task per commit, each with
//    its own durable output row — landed by a merge-queue task.
// 5. DETERMINISTIC CI GATE BEFORE REVIEW: review round 1's only blocker was cargo-stow —
//    a mechanical check discovered by the most expensive agent in the pipeline. A
//    compute ci-gate now runs the repo's own hooks (fmt, clippy, stow, biome, tooling
//    typecheck) before the reviewer ever mounts, so reviews spend on semantics only.
// 6. SMALLER DESIGN PANEL: three independent designs + synthesis was the priciest
//    phase and the third seat did not change the final plan (consistent with OrchBench:
//    panels are usually net-negative vs one strong design + adversarial review).
//    designSeats now defaults to 2 (Sol + Fable); pass designSeats: 3 to re-add Kimi.
// 7. ADAPTER/TOOLCHAIN PREFLIGHT: a design round was lost to a 4-major-versions-stale
//    OpenCode, and `typescript@latest` on npm is now the native Go build (no
//    tsserver.js). Preflight now probes adapter versions with minimums and records
//    toolchain pins for the run.
// 8. HISTORY SAFETY IN FIX ROUNDS: a fix-round instruction to "squash-style rework"
//    caused the scaffold commit to be dropped and the workflow files deleted from disk
//    mid-run. Fix rounds are now additive-commit only; history rewrites are reserved
//    for the dedicated commit-series step, which must keep the scaffold commit first.
//
/** @jsxImportSource smthrs */
import { Worktree, createSmithers } from "smthrs";
import { z } from "zod/v4";
import { TEVM_REPO_DEFAULT, setTevmRepo } from "../lib/constants.ts";
import { asBool, asStr } from "../lib/helpers.ts";
import { commitPlanOf } from "../lib/plan.ts";
import { CiGate, ciGateSchema } from "../tasks/ci-gate.tsx";
import { CleanupAndVerify, cleanupSchema } from "../tasks/cleanup-and-verify.tsx";
import { CommitReviews, commitReviewsSchema } from "../tasks/commit-reviews.tsx";
import { CommitSeries, commitsSchema } from "../tasks/commit-series.tsx";
import { CreateDraftPr, prSchema } from "../tasks/create-draft-pr.tsx";
import { DesignSeatTask, designSchema } from "../tasks/design.tsx";
import { claudeFable, codexSol, kimiOpenCode } from "../lib/agents.ts";
import { FinalizeDoc, docSchema } from "../tasks/finalize-doc.tsx";
import { Fix, fixSchema } from "../tasks/fix.tsx";
import { LaneImplementGuard, LegacyImplementGuard, guardSchema } from "../tasks/implement-guard.tsx";
import { Implement, implSchema } from "../tasks/implement.tsx";
import { LaneCommitTask, laneCommitSchema } from "../tasks/lane-commit.tsx";
import { MergeLanes, landSchema } from "../tasks/merge-lanes.tsx";
import { PanelFix, panelFixSchema } from "../tasks/panel-fix.tsx";
import { Preflight, preflightSchema } from "../tasks/preflight.tsx";
import { RecordDemos, demosSchema } from "../tasks/record-demos.tsx";
import { RecordPreSeriesHead, preSeriesSchema } from "../tasks/record-pre-series-head.tsx";
import { ResearchBaml, bamlResearchSchema } from "../tasks/research-baml.tsx";
import { ResearchTevm, researchSchema } from "../tasks/research-tevm.tsx";
import { ReviewSite, siteSchema } from "../tasks/review-site.tsx";
import { Review, reviewSchema } from "../tasks/review.tsx";
import { SignoffPanel, panelSchema, panelVerdictSchema } from "../tasks/signoff-panel.tsx";
import { SynthesizePlan, planSchema } from "../tasks/synthesize-plan.tsx";
import { VerifySeries, seriesCheckSchema } from "../tasks/verify-series.tsx";

// ── Input ────────────────────────────────────────────────────────────────────
const inputSchema = z.object({
  upstreamRepo: z.string().default("BoundaryML/baml"),
  baseBranch: z.string().default("canary"),
  featureBranch: z.string().default("feat/ts-language-tooling"),
  maxReviewRounds: z.number().int().min(1).max(8).default(4),
  // Retro change 6: default panel is two seats (Sol + Fable). The first run's third
  // seat did not change the final plan but did cost a full design task + an adapter
  // failure round. Pass 3 to re-add the Kimi/OpenCode seat.
  designSeats: z.number().int().min(2).max(3).default(2),
  // Bound for the final four-seat sign-off panel loop.
  maxPanelRounds: z.number().int().min(1).max(6).default(3),
  // Where the TEVM checkout lives (reproducibility): defaults to a temp-dir path
  // that preflight shallow-clones on demand; pass an existing checkout to reuse it.
  tevmRepo: z.string().default(TEVM_REPO_DEFAULT),
});

// ── Output tables ────────────────────────────────────────────────────────────
// Schemas live beside their task components in ../tasks/. Workflow-unique `btt`
// prefix so these tables can never collide with another workflow's tables in a
// shared workspace DB; required fields carry value constraints so an empty `{}`
// can never pass validation.
const { Workflow, Task, Sequence, Parallel, Loop, smithers, outputs } = createSmithers({
  input: inputSchema,
  bttPreflight: preflightSchema,
  bttTevmResearch: researchSchema,
  bttBamlResearch: bamlResearchSchema,
  bttDesign: designSchema,
  bttPlan: planSchema,
  bttImpl: implSchema,
  bttLane: laneCommitSchema,
  bttLand: landSchema,
  bttImplGuard: guardSchema,
  bttCiGate: ciGateSchema,
  bttReview: reviewSchema,
  bttFix: fixSchema,
  bttCommitReviews: commitReviewsSchema,
  bttDemos: demosSchema,
  bttSite: siteSchema,
  bttPanel: panelSchema,
  bttPanelVerdict: panelVerdictSchema,
  bttPanelFix: panelFixSchema,
  bttDoc: docSchema,
  bttPreSeries: preSeriesSchema,
  bttCommits: commitsSchema,
  bttSeriesCheck: seriesCheckSchema,
  bttPr: prSchema,
  bttCleanup: cleanupSchema,
});

// ── Workflow (thin composition) ──────────────────────────────────────────────
export default smithers((ctx) => {
  const input = {
    upstreamRepo: ctx.input?.upstreamRepo ?? "BoundaryML/baml",
    baseBranch: ctx.input?.baseBranch ?? "canary",
    featureBranch: ctx.input?.featureBranch ?? "feat/ts-language-tooling",
    maxReviewRounds: Number(ctx.input?.maxReviewRounds ?? 4) || 4,
    designSeats: Number(ctx.input?.designSeats ?? 2) || 2,
    maxPanelRounds: Number(ctx.input?.maxPanelRounds ?? 3) || 3,
    tevmRepo: String(ctx.input?.tevmRepo || TEVM_REPO_DEFAULT),
  };
  // Resolve the module-level TEVM path before any prompt builder runs.
  setTevmRepo(input.tevmRepo);

  const lastReview = ctx.latest(outputs.bttReview, "review");
  const reviewDone = Boolean(lastReview) && asBool(lastReview, "approved") && asBool(lastReview, "testsPassed");

  // Legacy compatibility: the first execution implemented everything in a single
  // `implement` node. If that row exists (a resumed legacy run), keep rendering the
  // legacy implement/guard shape; fresh runs use the commit-plan lanes (retro
  // change 4). Same trick for the third design seat: render it whenever its row
  // already exists so a resumed legacy run keeps a stable graph.
  const legacyImpl = Boolean(ctx.latest(outputs.bttImpl, "implement"));
  const kimiSeatActive =
    input.designSeats >= 3 || Boolean(ctx.outputMaybe(outputs.bttDesign, { nodeId: "design-kimi-opencode" }));

  const planRow = ctx.latest(outputs.bttPlan, "synthesize-plan");
  const commitPlan = commitPlanOf(planRow);
  const lanes = [...new Set(commitPlan.map((e) => e.lane))].sort((a, b) => a - b);
  const lastLaneNodeOf = (lane: number) => {
    const idxs = commitPlan.map((e, i) => ({ e, i })).filter(({ e }) => e.lane === lane);
    return `commit-${idxs[idxs.length - 1]?.i ?? 0}`;
  };

  // Final sign-off panel state: the <Panel> moderator (strategy "consensus",
  // minAgree 4) writes the ruling verdict row; it gates the loop and cleanup.
  const panelVerdictRow = ctx.latest(outputs.bttPanelVerdict, "signoff-moderator");
  const allLgtm = Boolean(panelVerdictRow) && asBool(panelVerdictRow, "allLgtm");

  return (
    <Workflow name="upstream-ts-tooling">
      {/* 0. Preflight: adapters, auth, fork, feature branch, scaffolding commit. */}
      <Preflight outputs={outputs} input={input} />

      {/* 1+2. Research: Codex Sol maps TEVM into a self-contained artifact, and
             independently maps current BAML. Explicit deps on a SUCCESSFUL
             preflight (an output row exists only on success), so ordering is
             carried by the graph itself, not just the implicit workflow
             sequence. */}
      <Parallel label="research">
        <ResearchTevm outputs={outputs} />
        <ResearchBaml outputs={outputs} />
      </Parallel>

      {/* 3. Independent design panel; every seat sees ONLY the two research
             artifacts (embedded verbatim into each prompt). */}
      <Parallel label="design-panel" maxConcurrency={3}>
        <DesignSeatTask outputs={outputs} seat="codex-sol" agent={codexSol} />
        <DesignSeatTask outputs={outputs} seat="claude-fable" agent={claudeFable} />
        {kimiSeatActive ? <DesignSeatTask outputs={outputs} seat="kimi-opencode" agent={kimiOpenCode} /> : null}
      </Parallel>

      {/* 4. Fable (Opus failover) synthesizes the designs into the exact final
             plan (incl. the file manifest and the tiered, laned commit plan). */}
      <SynthesizePlan outputs={outputs} upstreamRepo={input.upstreamRepo} kimiSeatActive={kimiSeatActive} />

      {/* 5. Implementation.
             LEGACY (resumed first run): the original single 8-hour implement node.
             FRESH (retro change 4 + user requirements): the commit plan's lanes run
             in parallel isolated worktrees — one task per atomic emoji commit, with
             cost-tiered agents (sol/terra/luna) — then a merge-queue task lands the
             lanes onto the feature branch in order. */}
      {legacyImpl
        ? [
            <Implement key="implement" outputs={outputs} />,
            <LegacyImplementGuard key="implement-guard" outputs={outputs} />,
          ]
        : [
            planRow && commitPlan.length === 0 ? (
              // The synthesized plan failed to include the machine-readable commit plan;
              // fail deterministically instead of silently doing nothing.
              <Task key="plan-guard" id="plan-guard" output={outputs.bttImplGuard} timeoutMs={60_000} noRetry>
                {() => {
                  throw new Error(
                    "synthesize-plan produced no commitPlan entries; re-run synthesis so the atomic commit plan exists before implementation.",
                  );
                }}
              </Task>
            ) : null,
            <Parallel key="commit-lanes" label="commit-lanes">
              {lanes.map((lane) => (
                <Worktree
                  key={`lane-${lane}`}
                  id={`lane-${lane}`}
                  path={`.smithers/worktrees/lane-${lane}`}
                  branch={`btt-lane-${lane}`}
                  baseBranch={input.featureBranch}
                >
                  <Sequence>
                    {commitPlan
                      .map((entry, index) => ({ entry, index }))
                      .filter(({ entry }) => entry.lane === lane)
                      .map(({ entry, index }, seq, laneEntries) => (
                        <LaneCommitTask
                          key={`commit-${index}`}
                          outputs={outputs}
                          entry={entry}
                          index={index}
                          prevNodeId={seq > 0 ? `commit-${laneEntries[seq - 1]?.index}` : undefined}
                        />
                      ))}
                  </Sequence>
                </Worktree>
              ))}
            </Parallel>,
            commitPlan.length > 0 ? (
              <MergeLanes
                key="merge-lanes"
                outputs={outputs}
                commitPlan={commitPlan}
                lanes={lanes}
                lastLaneNodeOf={lastLaneNodeOf}
                featureBranch={input.featureBranch}
              />
            ) : null,
            commitPlan.length > 0 ? (
              <LaneImplementGuard
                key="implement-guard"
                outputs={outputs}
                ctx={ctx}
                commitPlan={commitPlan}
                featureBranch={input.featureBranch}
              />
            ) : null,
          ]}

      {/* 5b. Deterministic CI gate (retro change 5): repo hooks before review. */}
      <CiGate outputs={outputs} />

      {/* 6. Bounded review loop: Fable (Opus failover) reviews, running the tests
             itself; Codex fixes; repeat until explicit approval AND passing tests,
             else the run fails. The review task explicitly depends on a SUCCESSFUL
             implement-guard (its output row exists only when the guard passed), so
             the first review can never mount or run before synthesize-plan →
             implement → implement-guard completed; the guard row is written at
             iteration 0 outside the loop, so later review rounds resolve the same
             dependency and stay correct. */}
      <Loop id="review-loop" until={reviewDone} maxIterations={input.maxReviewRounds} onMaxReached="fail">
        <Sequence>
          <Review outputs={outputs} ctx={ctx} input={input} />
          {(() => {
            const thisRound = ctx.outputMaybe(outputs.bttReview, { nodeId: "review" });
            const approvedNow = Boolean(thisRound) && asBool(thisRound, "approved") && asBool(thisRound, "testsPassed");
            return thisRound && !approvedNow ? <Fix outputs={outputs} ctx={ctx} /> : null;
          })()}
        </Sequence>
      </Loop>

      {/* 6b. Fable finalizes the maintainer doc and drafts the PR text. Mounted
              ONLY once the review loop produced an explicit approval with passing
              tests (`reviewDone` reads the latest review row): the conditional IS
              the dependency, so finalize-doc can never exist in the plan — let
              alone run — before the review-loop approval. A never-approving loop
              fails the run (onMaxReached="fail") and this task never mounts. */}
      {reviewDone ? <FinalizeDoc outputs={outputs} ctx={ctx} input={input} /> : null}

      {/* 7a. Record the exact pre-rewrite head so the history rewrite can be
             verified content-identical afterwards. */}
      <RecordPreSeriesHead outputs={outputs} />

      {/* 7b. Clean commit series (scaffolding commit stays first and untouched),
             then deterministic verification. */}
      <CommitSeries outputs={outputs} ctx={ctx} input={input} />
      <VerifySeries outputs={outputs} input={input} />

      {/* 7c. Push to the user's fork and open (or verify) the DRAFT PR. */}
      <CreateDraftPr outputs={outputs} input={input} />

      {/* 7d. Per-commit Sol review with an easy-to-review artifact per commit. */}
      <CommitReviews outputs={outputs} input={input} />

      {/* 7e. Real terminal proof: VHS recordings, published + embedded in the PR. */}
      <RecordDemos outputs={outputs} />

      {/* 7f. Commit-by-commit review site deployed to baml-unplugin-pr.smithers.sh. */}
      <ReviewSite outputs={outputs} input={input} />

      {/* 7g. FINAL SIGN-OFF PANEL via <Panel>: Kimi K3, Codex Sol, Claude Fable,
             and Claude Opus independently verify ALL work; a moderator rules with
             consensus (minAgree 4); Codex Sol fixes anything raised; loop until
             the moderator's verdict is allLgtm. */}
      {(() => {
        const prRowData = ctx.outputMaybe(outputs.bttPr, { nodeId: "create-draft-pr" });
        const siteRowData = ctx.outputMaybe(outputs.bttSite, { nodeId: "review-site" });
        const demosRowData = ctx.outputMaybe(outputs.bttDemos, { nodeId: "record-demos" });
        const docRowData = ctx.outputMaybe(outputs.bttDoc, { nodeId: "finalize-doc" });
        if (!prRowData || !siteRowData || !demosRowData) return null;
        const refs = {
          prUrl: asStr(prRowData, "prUrl"),
          siteUrl: asStr(siteRowData, "url"),
          docPath: asStr(docRowData, "docPath", "typescript2/docs/ts-tooling-architecture.md"),
        };
        return (
          <Loop id="final-panel-loop" until={allLgtm} maxIterations={input.maxPanelRounds} onMaxReached="fail">
            <Sequence>
              <SignoffPanel outputs={outputs} refs={refs} input={input} />
              {(() => {
                const verdict = ctx.outputMaybe(outputs.bttPanelVerdict, { nodeId: "signoff-moderator" });
                const rejectedNow = Boolean(verdict) && !asBool(verdict, "allLgtm");
                return rejectedNow ? <PanelFix outputs={outputs} ctx={ctx} /> : null;
              })()}
            </Sequence>
          </Loop>
        );
      })()}

      {/* 8. FINAL task: delete every piece of Smithers boilerplate (explicit
             enumerated list), commit + push, verify the draft PR includes the
             cleanup. Mounts only after the panel moderator's allLgtm verdict
             (the conditional IS the dependency). */}
      {allLgtm ? <CleanupAndVerify outputs={outputs} input={input} /> : null}
    </Workflow>
  );
});
