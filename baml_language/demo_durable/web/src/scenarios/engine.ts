/**
 * The scenario guide: what a scenario is, what the guide can see of the runs
 * that a scenario follows, and which step is the current one. Pure functions,
 * without React, so the hints and the autoplay decisions are unit tested.
 *
 * A scenario is a list of steps. A step is done when its `done` predicate
 * holds for the event history. The predicates read events that stay in the
 * history, so a step never becomes undone. A step whose moment has passed,
 * because a later step is already done, counts as skipped. This keeps the
 * guide in step with a user who acts early, late, or not at all.
 */

import type { FixtureName } from "../fixtures";
import { isLiveStatus, type JsonObject, type Run, type Site } from "../protocol";
import { runKey, type AppState, type RunAction, type RunKey, type TimelineEvent } from "../state";

export type ScenarioId = "migrate" | "fanout" | "race" | "deadline" | "settled" | "recover" | "fork";

/** The run of the scenario that a step reads or acts on: a started run by its role, or the first fork of the root run. */
export type Role = "root" | "plain" | "fork";

export interface RunStart {
  site: Site;
  fn: string;
  args: JsonObject;
}

export type StepAction =
  /** A control of the runs panel, on the current record of the run in `role`. */
  | { kind: "command"; role: Role; action: RunAction; site?: Site }
  /** Starting another run, which takes the role `as`. */
  | { kind: "start"; as: Exclude<Role, "root" | "fork">; start: RunStart };

export interface ScenarioStep {
  id: string;
  /** Two or three words for the step list. */
  title: string;
  /** The hint while the step is current. For a step with an action: while the action is possible. */
  hint(view: ScenarioView): string;
  /** For a step with an action: the hint while the action is not possible yet. */
  waitHint?(view: ScenarioView): string;
  action?: StepAction;
  /** For a step with an action: whether the action is possible and sensible now. Default: always. */
  ready?(view: ScenarioView): boolean;
  /** The user may leave the step out. The guide says so. Autoplay performs it. */
  optional?: boolean;
  done(view: ScenarioView): boolean;
}

export interface Scenario {
  id: ScenarioId;
  title: string;
  /** One paragraph in plain words. */
  summary: string;
  /** What the scenario shows, as short tags. */
  shows: string[];
  start: RunStart;
  /** The recording that plays the scenario without servers. */
  fixture: FixtureName;
  steps: ScenarioStep[];
  /** The closing line once every step is done or skipped. */
  outcome(view: ScenarioView): string;
}

// ---------------------------------------------------------------------------
// The view of a scenario's runs
// ---------------------------------------------------------------------------

export interface RoleView {
  id: string;
  /** Every record of the run id. A run that moved has one per site. */
  records: Run[];
  /** The record that is not `migrated`, or the latest one. `null` until a record arrived. */
  current: Run | null;
  /** The events of the run on every site, in time order. */
  events: TimelineEvent[];
  /** The remote children of the run, in call order. */
  children: Run[];
}

export interface ScenarioView {
  root: RoleView | null;
  plain: RoleView | null;
  fork: RoleView | null;
  now: number;
}

export type RoleRefs = Partial<Record<Exclude<Role, "fork">, { site: Site; id: string }>>;

function roleView(state: Pick<AppState, "runs" | "events">, id: string): RoleView {
  const records = Object.values(state.runs).filter((run) => run.id === id);
  const live = records.filter((run) => run.status !== "migrated");
  const current = [...(live.length > 0 ? live : records)].sort((a, b) => a.segment - b.segment || a.updated_ts - b.updated_ts).pop() ?? null;
  const events: TimelineEvent[] = [];
  for (const [key, list] of Object.entries(state.events)) {
    if (key.slice(key.indexOf("/") + 1) === id) events.push(...list);
  }
  events.sort((a, b) => a.ts - b.ts);
  const children = Object.values(state.runs)
    .filter((run) => run.parent?.run === id)
    .sort((a, b) => a.created_ts - b.created_ts || a.id.localeCompare(b.id));
  return { id, records, current, events, children };
}

export function scenarioView(state: Pick<AppState, "runs" | "events">, roles: RoleRefs, now: number): ScenarioView {
  const root = roles.root ? roleView(state, roles.root.id) : null;
  const plain = roles.plain ? roleView(state, roles.plain.id) : null;
  const forked = root ? Object.values(state.runs).filter((run) => run.forked_from?.run === root.id).sort((a, b) => a.created_ts - b.created_ts)[0] : undefined;
  return { root, plain, fork: forked ? roleView(state, forked.id) : null, now };
}

// Helpers for the step predicates. Each one tolerates a role that has no run yet.

export function eventsOf<T extends TimelineEvent["type"]>(role: RoleView | null, type: T): Extract<TimelineEvent, { type: T }>[] {
  return (role?.events ?? []).filter((event): event is Extract<TimelineEvent, { type: T }> => event.type === type);
}

export function statusOf(role: RoleView | null): Run["status"] | null {
  return role?.current?.status ?? null;
}

export function isTerminal(role: RoleView | null): boolean {
  const status = statusOf(role);
  return status === "completed" || status === "failed" || status === "cancelled" || status === "lost";
}

export function childrenWith(role: RoleView | null, ...statuses: Run["status"][]): Run[] {
  return (role?.children ?? []).filter((run) => statuses.includes(run.status));
}

export function runningChildren(role: RoleView | null): Run[] {
  return (role?.children ?? []).filter((run) => isLiveStatus(run.status));
}

/** "cloud and cloud2": the sites of `runs`, without repeats, in order of appearance. */
export function sitesOf(runs: readonly Run[]): string {
  const sites = [...new Set(runs.map((run) => run.site))];
  return sites.length <= 1 ? (sites[0] ?? "the pool") : `${sites.slice(0, -1).join(", ")} and ${sites[sites.length - 1]}`;
}

const NUMBER_WORDS = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight"];
export const countWord = (count: number): string => NUMBER_WORDS[count] ?? String(count);

/** The number of process starts of a run: its `hello` events. */
export function segmentsStarted(role: RoleView | null): number {
  return eventsOf(role, "hello").length;
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

export type StepState = "done" | "skipped" | "current" | "todo";

export interface ResolvedAction {
  step: string;
  action: StepAction;
  /** For a command: the record to select and act on. `null` while the run has no record. */
  target: RunKey | null;
}

export interface ScenarioProgress {
  /** One entry per step. */
  states: StepState[];
  /** Index of the current step, or `steps.length` when the scenario is over. */
  index: number;
  finished: boolean;
  hint: string;
  /** The action of the current step. `null` for a step that only watches, and at the end. */
  action: ResolvedAction | null;
  /** The action is possible now: the guide points at its control, and autoplay presses it. */
  ready: boolean;
  optional: boolean;
}

export function scenarioProgress(scenario: Scenario, view: ScenarioView): ScenarioProgress {
  const done = scenario.steps.map((step) => step.done(view));
  const lastDone = done.lastIndexOf(true);
  const index = lastDone + 1;
  const states = scenario.steps.map((_, i): StepState => (i < index ? (done[i] ? "done" : "skipped") : i === index ? "current" : "todo"));
  const step = scenario.steps[index];
  if (!step) {
    return { states, index, finished: true, hint: scenario.outcome(view), action: null, ready: false, optional: false };
  }
  let action: ResolvedAction | null = null;
  let ready = false;
  if (step.action) {
    ready = step.ready ? step.ready(view) : true;
    const role = step.action.kind === "command" ? view[step.action.role] : null;
    const record = role?.current ?? null;
    action = { step: step.id, action: step.action, target: record ? runKey(record.site, record.id) : null };
    if (step.action.kind === "command" && record === null) ready = false;
  }
  const hint = step.action && !ready && step.waitHint ? step.waitHint(view) : step.hint(view);
  return { states, index, finished: false, hint, action, ready, optional: step.optional ?? false };
}
