// Validation harness for upstream-ts-tooling.tsx (workflow scaffolding; removed by
// the workflow's final cleanup task together with the workflow script).
//
// Proves, against the real graph-extraction path (`renderWorkflow`, the same path
// `smithers graph` and `smithers up` use), that tasks mount frame by frame in the
// intended dependency order — in particular the two reviewer findings:
//   1. `review` cannot exist in any frame before a SUCCESSFUL `implement-guard`
//      row exists (and later review rounds still resolve the same dependency);
//   2. `finalize-doc` cannot exist in any frame before the review loop produced
//      an explicit approval with passing tests.
// A full stubbed execution (`coverWorkflow`) then proves end-to-end execution
// order without running any real agent, git, or gh command.
//
// Run with the WORKING-TREE Smithers runtime resolution (.smithers/node_modules
// forwards smthrs into /Users/williamcory/smithers). MUST run with .smithers as
// the cwd so .smithers/bunfig.toml's mdxPlugin preload applies (the workflow's
// prompts are *.mdx modules). From the baml repo root:
//   bun run --cwd .smithers test
/** @jsxImportSource smthrs */
import { describe, expect, test } from "bun:test";
import { coverWorkflow, renderWorkflow } from "smthrs/testing";
import workflow from "./upstream-ts-tooling.tsx";

const RUN_ID = "validation-run";
const pad = (label: string, len: number) => label.padEnd(len, " lorem ipsum dolor sit amet").slice(0, Math.max(len, label.length));

type Row = Record<string, unknown>;
const row = (nodeId: string, payload: Row, iteration = 0): Row => ({
  runId: RUN_ID,
  nodeId,
  iteration,
  ...payload,
});

// Schema-valid payloads for every upstream row a later frame depends on.
const preflightRow = row("preflight", {
  login: "octocat",
  forkUrl: "https://github.com/octocat/baml.git",
  branch: "feat/ts-language-tooling",
  scaffoldCommit: "abcdef1234567890",
  toolVersions: ["codex 1", "claude 1", "opencode 1", "gh 1", "git 1"],
  notes: "all adapters present",
});
const researchRow = (nodeId: string) =>
  row(nodeId, {
    artifactPath: `.smithers/artifacts/upstream-ts-tooling/${nodeId}.md`,
    summary: pad("research summary", 220),
    sectionsCovered: ["a", "b", "c", "d", "e", "f"],
  });
const designRow = (nodeId: string, seat: string) =>
  row(nodeId, {
    seat,
    artifactPath: `.smithers/artifacts/upstream-ts-tooling/design-${seat}.md`,
    summary: pad("design summary", 220),
    keyDecisions: ["one", "two", "three"],
    risks: [],
  });
const planRow = row("synthesize-plan", {
  artifactPath: ".smithers/artifacts/upstream-ts-tooling/final-plan.md",
  summary: pad("plan summary", 220),
  packageBoundaries: pad("boundaries", 120),
  languageServiceBehavior: pad("ls behavior", 120),
  sourceMapStrategy: pad("source maps", 60),
  testPlan: pad("test plan", 120),
  rolloutPlan: pad("rollout", 60),
  disagreementsResolved: ["ruling one"],
});
const implRow = row("implement", {
  status: "complete",
  summary: pad("implementation summary", 220),
  filesChanged: ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts"],
  testCommands: ["cargo test --lib"],
  docDraftPath: "docs/ts-tooling.md",
  blockedReason: "",
});
const guardRow = row("implement-guard", { ok: true, note: "implementation complete" });
const ciGateRow = row("ci-gate", {
  ok: true,
  checks: ["cargo fmt --check: PASS", "cargo-stow workspace invariants: PASS", "biome check: PASS"],
  notes: "mechanical repo gates green before review",
});
const approvedReviewRow = row("review", {
  approved: true,
  testsPassed: true,
  testEvidence: pad("all suites green, decisive output attached", 60),
  blockingIssues: [],
  feedback: pad("looks good; approved", 30),
});
const docRow = row("finalize-doc", {
  docPath: "docs/ts-tooling.md",
  prTitle: "feat: TEVM-grade TypeScript tooling",
  prBody: pad("pr body", 220),
  summary: pad("doc summary", 60),
});

const ids = (frame: { tasks: readonly { nodeId: string }[] }) => frame.tasks.map((t) => t.nodeId);

describe("upstream-ts-tooling graph mounts frames in dependency order", () => {
  test("frame 1: only preflight exists; nothing downstream is in the plan", async () => {
    const frame = await renderWorkflow(workflow, { runId: RUN_ID, input: {} });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("preflight");
    for (const later of [
      "research-tevm",
      "research-baml",
      "design-codex-sol",
      "design-claude-fable",
      "design-kimi-opencode",
      "synthesize-plan",
      "implement",
      "implement-guard",
      "review",
      "fix",
      "finalize-doc",
      "record-pre-series-head",
      "commit-series",
      "verify-series",
      "create-draft-pr",
      "cleanup-and-verify",
    ]) {
      expect(nodeIds).not.toContain(later);
    }
  });

  test("implement row alone mounts the guard but never the first review", async () => {
    const frame = await renderWorkflow(workflow, {
      runId: RUN_ID,
      input: {},
      outputs: {
        bttPreflight: [preflightRow],
        bttTevmResearch: [researchRow("research-tevm")],
        bttBamlResearch: [researchRow("research-baml")],
        bttDesign: [
          designRow("design-codex-sol", "codex-sol"),
          designRow("design-claude-fable", "claude-fable"),
          designRow("design-kimi-opencode", "kimi-opencode"),
        ],
        bttPlan: [planRow],
        bttImpl: [implRow],
      },
    });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("implement-guard");
    expect(nodeIds).not.toContain("review");
    expect(nodeIds).not.toContain("finalize-doc");
  });

  test("a successful implement-guard row mounts ci-gate and review; finalize-doc still absent", async () => {
    const frame = await renderWorkflow(workflow, {
      runId: RUN_ID,
      input: {},
      outputs: {
        bttPreflight: [preflightRow],
        bttTevmResearch: [researchRow("research-tevm")],
        bttBamlResearch: [researchRow("research-baml")],
        bttDesign: [
          designRow("design-codex-sol", "codex-sol"),
          designRow("design-claude-fable", "claude-fable"),
          designRow("design-kimi-opencode", "kimi-opencode"),
        ],
        bttPlan: [planRow],
        bttImpl: [implRow],
        bttImplGuard: [guardRow],
        bttCiGate: [ciGateRow],
      },
    });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("ci-gate");
    expect(nodeIds).toContain("review");
    expect(nodeIds).not.toContain("finalize-doc");
    expect(nodeIds).not.toContain("record-pre-series-head");
  });

  test("an approved review mounts finalize-doc; pre-series recording waits for the doc row", async () => {
    const frame = await renderWorkflow(workflow, {
      runId: RUN_ID,
      input: {},
      outputs: {
        bttPreflight: [preflightRow],
        bttTevmResearch: [researchRow("research-tevm")],
        bttBamlResearch: [researchRow("research-baml")],
        bttDesign: [
          designRow("design-codex-sol", "codex-sol"),
          designRow("design-claude-fable", "claude-fable"),
          designRow("design-kimi-opencode", "kimi-opencode"),
        ],
        bttPlan: [planRow],
        bttImpl: [implRow],
        bttImplGuard: [guardRow],
        bttReview: [approvedReviewRow],
      },
    });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("finalize-doc");
    expect(nodeIds).not.toContain("record-pre-series-head");
    expect(nodeIds).not.toContain("commit-series");
  });

  test("a later review round still resolves the iteration-0 guard row (loop iteration 1)", async () => {
    const frame = await renderWorkflow(workflow, {
      runId: RUN_ID,
      input: {},
      iterations: { "review-loop": 1 },
      outputs: {
        bttPreflight: [preflightRow],
        bttTevmResearch: [researchRow("research-tevm")],
        bttBamlResearch: [researchRow("research-baml")],
        bttDesign: [
          designRow("design-codex-sol", "codex-sol"),
          designRow("design-claude-fable", "claude-fable"),
          designRow("design-kimi-opencode", "kimi-opencode"),
        ],
        bttPlan: [planRow],
        bttImpl: [implRow],
        bttImplGuard: [guardRow],
        bttCiGate: [ciGateRow],
        bttReview: [
          row("review", { ...approvedReviewRow, approved: false, testsPassed: false }, 0),
          // no iteration-1 review row yet: round 2's review must still mount
        ],
        bttFix: [row("fix", { summary: pad("fix summary", 60), addressed: ["issue"] }, 0)],
      },
    });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("review");
    expect(nodeIds).not.toContain("finalize-doc");
  });

  test("the doc row mounts pre-series recording; commit-series waits for the recorded head", async () => {
    const frame = await renderWorkflow(workflow, {
      runId: RUN_ID,
      input: {},
      outputs: {
        bttPreflight: [preflightRow],
        bttTevmResearch: [researchRow("research-tevm")],
        bttBamlResearch: [researchRow("research-baml")],
        bttDesign: [
          designRow("design-codex-sol", "codex-sol"),
          designRow("design-claude-fable", "claude-fable"),
          designRow("design-kimi-opencode", "kimi-opencode"),
        ],
        bttPlan: [planRow],
        bttImpl: [implRow],
        bttImplGuard: [guardRow],
        bttReview: [approvedReviewRow],
        bttDoc: [docRow],
      },
    });
    const nodeIds = ids(frame);
    expect(nodeIds).toContain("record-pre-series-head");
    expect(nodeIds).not.toContain("commit-series");
    expect(nodeIds).not.toContain("verify-series");
  });
});

describe("upstream-ts-tooling stubbed end-to-end execution order (fresh run: lanes + panel)", () => {
  test("every stage executes strictly after its dependency; fix tasks never run when approved", async () => {
    const laneDone = (sha: string, subject: string) => ({
      status: "complete",
      commitSha: sha,
      subject,
      validationEvidence: pad("validation commands ran and passed, decisive output attached", 60),
      summary: pad("commit implemented", 60),
      blockedReason: "",
    });
    const stackPrDone = (prNumber: number, branch: string) => ({
      prNumber,
      prUrl: `https://github.com/BoundaryML/baml/pull/${prNumber}`,
      branch,
      headSha: "abcdef1234567",
      draft: true,
      summary: pad("stacked draft PR pushed and created/updated", 40),
    });
    const panelLgtm = (seat: string) => ({
      seat,
      lgtm: true,
      issues: [],
      notes: pad(`verified everything as ${seat}`, 30),
    });
    const result = await coverWorkflow(workflow, {
      input: {},
      mocks: {
        "synthesize-plan": {
          artifactPath: ".smithers/artifacts/upstream-ts-tooling/final-plan.md",
          summary: pad("plan summary", 220),
          packageBoundaries: pad("boundaries", 120),
          languageServiceBehavior: pad("ls behavior", 120),
          sourceMapStrategy: pad("source maps", 60),
          testPlan: pad("test plan", 120),
          rolloutPlan: pad("rollout", 60),
          disagreementsResolved: ["ruling one"],
          commitPlan: [
            {
              emoji: "✨",
              subject: "feat(tooling): add core crate",
              rationale: pad("the core crate carries the semantics", 24),
              files: ["baml_language/crates/baml_tooling_api/src/lib.rs"],
              validation: ["cargo test -p baml_tooling_api --lib"],
              tier: "sol",
              lane: 0,
              pr: 1,
            },
            {
              emoji: "✅",
              subject: "test(tooling): cover the TS package",
              rationale: pad("proof the TS surface works", 24),
              files: ["typescript2/pkg-baml-tooling/src/index.ts"],
              validation: ["pnpm --filter @boundaryml/baml-tooling test"],
              tier: "luna",
              lane: 1,
              pr: 2,
            },
          ],
        },
        "commit-0-base": { baseHash: "1234567890abcd", tool: "git" },
        "commit-1-base": { baseHash: "234567890abcde", tool: "git" },
        "pr-1-base": { baseHash: "34567890abcdef", tool: "git" },
        "pr-2-base": { baseHash: "4567890abcdef1", tool: "git" },
        "commit-0-commit": laneDone("abcdef1234567", "feat(tooling): add core crate"),
        "commit-1-commit": laneDone("fedcba7654321", "test(tooling): cover the TS package"),
        "pr-1-pr": stackPrDone(101, "feat/ts-language-tooling-pr-1"),
        "pr-2-pr": stackPrDone(102, "feat/ts-language-tooling-pr-2"),
        review: {
          approved: true,
          testsPassed: true,
          testEvidence: pad("all suites green, decisive output attached", 60),
          blockingIssues: [],
          feedback: pad("looks good; approved", 30),
        },
        "signoff-kimi": panelLgtm("kimi-k3"),
        "signoff-sol": panelLgtm("codex-sol"),
        "signoff-fable": panelLgtm("claude-fable"),
        "signoff-opus": panelLgtm("claude-opus"),
        "signoff-moderator": {
          allLgtm: true,
          summary: pad("all four seats independently verified and LGTMed the deliverables", 60),
          outstanding: [],
        },
      },
      maxLoopIterations: 2,
      assert: false,
    });

    const executed = result.executed;
    const at = (nodeId: string) => {
      const index = executed.indexOf(nodeId);
      expect(index).toBeGreaterThanOrEqual(0);
      return index;
    };

    // Whole pipeline, in dependency order. Fresh runs default to TWO design seats.
    expect(at("preflight")).toBeLessThan(at("research-tevm"));
    expect(at("preflight")).toBeLessThan(at("research-baml"));
    for (const design of ["design-codex-sol", "design-claude-fable"]) {
      expect(at("research-tevm")).toBeLessThan(at(design));
      expect(at("research-baml")).toBeLessThan(at(design));
      expect(at(design)).toBeLessThan(at("synthesize-plan"));
    }
    expect(executed).not.toContain("design-kimi-opencode");
    // Stacked-PR delivery: every planned commit executes after synthesis, after
    // its base-hash snapshot, and before its own PR's pr task; PR 1's pr task
    // runs before PR 2's (its base branch must exist on the fork first); the
    // guard runs only after the whole stack.
    for (const commit of ["commit-0-commit", "commit-1-commit"]) {
      expect(at("synthesize-plan")).toBeLessThan(at(commit));
    }
    expect(at("commit-0-base")).toBeLessThan(at("commit-0-commit"));
    expect(at("commit-1-base")).toBeLessThan(at("commit-1-commit"));
    expect(at("commit-0-commit")).toBeLessThan(at("pr-1-pr"));
    expect(at("commit-1-commit")).toBeLessThan(at("pr-2-pr"));
    expect(at("pr-1-pr")).toBeLessThan(at("pr-2-pr"));
    expect(at("pr-1-pr")).toBeLessThan(at("commit-1-commit"));
    expect(at("pr-1-pr")).toBeLessThan(at("implement-guard"));
    expect(at("pr-2-pr")).toBeLessThan(at("implement-guard"));
    // The legacy single-PR and lane machinery never runs on a fresh stack run.
    expect(executed).not.toContain("create-draft-pr");
    expect(executed).not.toContain("merge-lanes");
    // Mechanical CI gate strictly between the guard and the first review.
    expect(at("implement-guard")).toBeLessThan(at("ci-gate"));
    expect(at("ci-gate")).toBeLessThan(at("review"));
    expect(at("review")).toBeLessThan(at("finalize-doc"));
    expect(at("finalize-doc")).toBeLessThan(at("record-pre-series-head"));
    expect(at("record-pre-series-head")).toBeLessThan(at("commit-series"));
    expect(at("commit-series")).toBeLessThan(at("verify-series"));
    expect(at("verify-series")).toBeLessThan(at("commit-reviews"));
    expect(at("verify-series")).toBeLessThan(at("record-demos"));
    expect(at("pr-2-pr")).toBeLessThan(at("record-demos"));
    expect(at("commit-reviews")).toBeLessThan(at("review-site"));
    // Final sign-off panel runs only after the PR, demos, and site all exist;
    // every seat precedes the moderator, whose allLgtm verdict gates cleanup.
    for (const seat of ["signoff-kimi", "signoff-sol", "signoff-fable", "signoff-opus"]) {
      expect(at("pr-2-pr")).toBeLessThan(at(seat));
      expect(at("record-demos")).toBeLessThan(at(seat));
      expect(at("review-site")).toBeLessThan(at(seat));
      expect(at(seat)).toBeLessThan(at("signoff-moderator"));
    }
    expect(at("signoff-moderator")).toBeLessThan(at("cleanup-and-verify"));

    // Approved rounds never run either fixer.
    expect(executed).not.toContain("fix");
    expect(executed).not.toContain("panel-fix");
    expect(result.status).toBe("finished");
  });
});
