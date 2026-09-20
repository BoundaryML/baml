import { describe, expect, it } from "vitest";
import { isFixtureName, loadFixture, type Fixture } from "../fixtures";
import { FIXTURE_EPOCH } from "../fixtures/builder";
import { BRANCH_FORK } from "../fixtures/branch";
import { normalizeRun, type Run } from "../protocol";
import { initialState, reducer, runKey, validActions, type AppState } from "../state";
import { playFixture } from "../testing";
import { SCENARIOS, scenarioById, scenarioOfFixture } from "./catalog";
import { scenarioProgress, scenarioView, type Scenario, type ScenarioProgress } from "./engine";

const at = (ms: number): number => FIXTURE_EPOCH + ms;

function progressAt(scenario: Scenario, fixture: Fixture, ms: number): ScenarioProgress {
  const state = playFixture(fixture, at(ms));
  return scenarioProgress(scenario, scenarioView(state, fixture.roles ?? {}, at(ms)));
}

/** The ids of the steps that were current at some time of the recording, in order, and the actions that were ready. */
function walk(scenario: Scenario, fixture: Fixture): { steps: string[]; acted: string[]; final: ScenarioProgress; state: AppState } {
  const steps: string[] = [];
  const acted: string[] = [];
  let state = initialState();
  let final: ScenarioProgress | null = null;
  for (const event of fixture.events) {
    state = reducer(state, { type: "sse", site: event.site, event });
    const progress = scenarioProgress(scenario, scenarioView(state, fixture.roles ?? {}, event.ts));
    const id = scenario.steps[progress.index]?.id ?? "done";
    if (steps[steps.length - 1] !== id) steps.push(id);
    if (progress.ready && progress.action && acted[acted.length - 1] !== progress.action.step) acted.push(progress.action.step);
    // A step never becomes undone: the index never goes back.
    if (final) expect(progress.index, `${scenario.id} at ${event.ts - FIXTURE_EPOCH}`).toBeGreaterThanOrEqual(final.index);
    final = progress;
  }
  if (!final) throw new Error("the fixture has no events");
  return { steps, acted, final, state };
}

describe("scenario catalog", () => {
  it("has the seven scenarios of the contract, each with a recording that names its root run", () => {
    expect(SCENARIOS.map((scenario) => scenario.id)).toEqual(["migrate", "fanout", "race", "deadline", "settled", "recover", "fork"]);
    for (const scenario of SCENARIOS) {
      expect(isFixtureName(scenario.fixture)).toBe(true);
      const fixture = loadFixture(scenario.fixture);
      expect(fixture.roles?.root, scenario.id).toBeDefined();
      expect(scenarioOfFixture(scenario.fixture)).toBe(scenario);
      // The recording starts the function that the scenario starts, on the same site, with the same arguments.
      const root = playFixture(fixture).runs[runKey(fixture.roles?.root?.site ?? "", fixture.roles?.root?.id ?? "")];
      expect([root?.function, root?.args, fixture.roles?.root?.site]).toEqual([scenario.start.fn, scenario.start.args, scenario.start.site]);
      expect(scenario.summary.length).toBeGreaterThan(120);
      expect(new Set(scenario.steps.map((step) => step.id)).size).toBe(scenario.steps.length);
    }
    expect(scenarioById("race")?.title).toBe("Race and cancellation");
    expect(scenarioById("nope")).toBeNull();
    // A fixture that no scenario plays has no guide.
    expect(scenarioOfFixture("central")).toBeNull();
  });

  it("walks every step of every scenario in order over its recording, and ends finished", () => {
    for (const scenario of SCENARIOS) {
      const { steps, final } = walk(scenario, loadFixture(scenario.fixture));
      expect(steps, scenario.id).toEqual([...scenario.steps.map((step) => step.id), "done"]);
      expect(final.finished).toBe(true);
      expect(final.states.every((state) => state === "done"), scenario.id).toBe(true);
      expect(final.hint.startsWith("Done.")).toBe(true);
      expect(final.action).toBeNull();
    }
  });

  it("offers every action at a moment at which the command is valid for the run", () => {
    for (const scenario of SCENARIOS) {
      const fixture = loadFixture(scenario.fixture);
      let state = initialState();
      for (const event of fixture.events) {
        state = reducer(state, { type: "sse", site: event.site, event });
        const progress = scenarioProgress(scenario, scenarioView(state, fixture.roles ?? {}, event.ts));
        if (!progress.ready || !progress.action || progress.action.action.kind !== "command") continue;
        expect(progress.action.target, `${scenario.id}/${progress.action.step}`).not.toBeNull();
        const run = state.runs[progress.action.target as keyof AppState["runs"]];
        expect(validActions(run)[progress.action.action.action], `${scenario.id}/${progress.action.step} on a ${run?.status} run`).toBe(true);
      }
    }
  });

  it("names the actions that the user, or autoplay, performs in each recording", () => {
    const acted = Object.fromEntries(SCENARIOS.map((scenario) => [scenario.id, walk(scenario, loadFixture(scenario.fixture)).acted]));
    expect(acted).toEqual({
      migrate: ["pause", "move"],
      fanout: [],
      race: ["pause", "resume"],
      deadline: ["pause", "resume"],
      settled: ["pause", "resume"],
      recover: ["kill", "resume", "plain", "kill-plain"],
      fork: ["pause", "fork", "resume-source", "resume-fork"],
    });
  });
});

describe("hints follow the event stream", () => {
  const fanout = scenarioById("fanout") as Scenario;
  const race = scenarioById("race") as Scenario;

  it("counts the children and the wake time down while the fan-out sleeps", () => {
    const fixture = loadFixture("fanout");
    expect(progressAt(fanout, fixture, 70).hint).toContain("one of four started");
    const sleeping = progressAt(fanout, fixture, 3500);
    expect(fanout.steps[sleeping.index]?.id).toBe("sleep");
    expect(sleeping.hint).toBe(
      "Watch: no parent process exists while the run sleeps. 2 of 4 children have finished on their machines, and their results are stored. The timer wakes the run in 8.6 s.",
    );
    expect(sleeping.action).toBeNull();
    expect(progressAt(fanout, fixture, 12_140).hint).toContain("The timer woke the run");
  });

  it("asks for the pause when the children of the race run, and names their sites", () => {
    const fixture = loadFixture("race");
    const waiting = progressAt(race, fixture, 80);
    expect(race.steps[waiting.index]?.id).toBe("spawn");
    const ready = progressAt(race, fixture, 1000);
    expect(ready).toMatchObject({ ready: true, optional: true, hint: "Click Pause now: three children are running on cloud and cloud2. The race survives the pause." });
    expect(ready.action).toMatchObject({ step: "pause", target: runKey("local", "r-rc3w8n"), action: { kind: "command", action: "pause" } });
    // While the snapshot is being written the control is not offered.
    const writing = progressAt(race, fixture, 1305);
    expect(writing).toMatchObject({ ready: false, hint: "The worker writes the snapshot at its next clean point and exits." });
    // Paused: the hint knows whether the winner has answered.
    expect(progressAt(race, fixture, 2000).hint).toContain("four pending futures. The children keep running");
    const stored = progressAt(race, fixture, 3000);
    expect(stored.hint).toContain("answered while no parent process existed");
    expect(stored.action?.action).toMatchObject({ action: "resume_on", site: "cloud2" });
  });

  it("targets the fork, not the source, when the fork is to be resumed", () => {
    const scenario = scenarioById("fork") as Scenario;
    const fixture = loadFixture("branch");
    const progress = progressAt(scenario, fixture, 5000);
    expect(progress.action).toMatchObject({ step: "resume-fork", target: runKey("local", BRANCH_FORK) });
    expect(progress.ready).toBe(true);
  });

  it("offers the start of the plain run once the durable run has completed, and then its kill", () => {
    const scenario = scenarioById("recover") as Scenario;
    const fixture = loadFixture("recover");
    const start = progressAt(scenario, fixture, 10_500);
    expect(start.action?.action).toEqual({ kind: "start", as: "plain", start: { site: "local", fn: "plan_trip", args: { city: "Lisbon" } } });
    expect(start.ready).toBe(true);
    const kill = progressAt(scenario, fixture, 11_400);
    expect(kill.action).toMatchObject({ step: "kill-plain", target: runKey("local", "r-rp4n7t") });
    expect(walk(scenario, fixture).final.hint).toContain("The plain run is lost");
  });
});

describe("a user who does not follow the guide", () => {
  const race = scenarioById("race") as Scenario;

  it("skips the optional pause and the resume when the race settles without them", () => {
    // The recording without the pause, the migration, and the second segment: the race settles in the first process.
    const fixture = loadFixture("race");
    const root = fixture.roles?.root?.id ?? "";
    let state = initialState();
    const run = (patch: Partial<Run>): Run => normalizeRun({ id: root, site: "local", function: "durable_race", durable: true, status: "running", segment: 1, ...patch } as Run);
    const send = (event: Record<string, unknown> & { type: string; ts: number }): void => {
      state = reducer(state, { type: "sse", site: "local", event: { site: "local", run: root, v: 1, segment: 1, pid: 5, ...event } as never });
    };
    state = reducer(state, { type: "sse", site: "local", event: { type: "run", ts: at(0), site: "local", run: run({ updated_ts: at(0) }) } });
    send({ type: "hello", ts: at(10), mode: "start", function: "durable_race", durable: true });
    for (const [index, child] of ["a", "b", "c"].entries()) {
      const record = normalizeRun({ id: `r-${child}`, site: index === 1 ? "cloud2" : "cloud", status: index === 0 ? "completed" : "running", created_ts: at(20 + index), parent: { site: "local", run: root, call_id: `${root}-c${index + 1}` } } as Run);
      state = reducer(state, { type: "sse", site: record.site, event: { type: "run", ts: at(20 + index), site: record.site, run: record } });
    }
    const before = scenarioProgress(race, scenarioView(state, { root: { site: "local", id: root } }, at(100)));
    expect(race.steps[before.index]?.id).toBe("pause");
    send({ type: "remote_cancel", ts: at(2600), call_id: `${root}-c2`, thread: 3 });
    const after = scenarioProgress(race, scenarioView(state, { root: { site: "local", id: root } }, at(2600)));
    expect(after.states).toEqual(["done", "skipped", "skipped", "done", "current"]);
    expect(after.action).toBeNull();
  });

  it("has no target and is not ready while the run has no record yet", () => {
    const migrate = scenarioById("migrate") as Scenario;
    const progress = scenarioProgress(migrate, scenarioView(initialState(), { root: { site: "local", id: "r-new" } }, at(0)));
    expect(progress).toMatchObject({ index: 0, ready: false, finished: false, hint: "The run starts on local. Wait for its first loop iteration." });
    expect(progress.action?.target).toBeNull();
  });
});
