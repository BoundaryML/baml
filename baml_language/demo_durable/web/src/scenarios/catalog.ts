/**
 * The scenarios of the gallery (contract section 9.6). Each one starts a
 * function of the demo program and guides the user through it with hints that
 * follow the event stream. The texts state what happens in plain words.
 */

import { formatBytes, formatCountdown } from "../format";
import {
  childrenWith,
  countWord,
  eventsOf,
  isTerminal,
  runningChildren,
  segmentsStarted,
  sitesOf,
  statusOf,
  type RoleView,
  type Scenario,
  type ScenarioView,
} from "./engine";

const CITY = { city: "Lisbon" };

const paused = (role: RoleView | null): boolean => eventsOf(role, "paused").length > 0;
const completed = (role: RoleView | null): boolean => statusOf(role) === "completed";
const planned = (role: RoleView | null): number => eventsOf(role, "log").filter((event) => event.text.startsWith("planning day")).length;
const cancels = (role: RoleView | null): number => Math.max(eventsOf(role, "remote_cancel").length, eventsOf(role, "remote_cancelled").length);
const latestSnapshot = (role: RoleView | null) => role?.current?.snapshots[role.current.snapshots.length - 1] ?? null;
/** A process that ended without a terminal event: a signal, or an exit code that the worker contract does not define. */
const lostProcess = (role: RoleView | null): boolean =>
  eventsOf(role, "worker_exit").some((event) => event.signal !== null || ![0, 1, 75, 130].includes(event.exit_code));

/** The hint of a pause step whose control is not offered: before the moment, or while the worker answers the request. */
const pauseWait = (view: ScenarioView, before: string): string =>
  statusOf(view.root) === "pausing" ? "The worker writes the snapshot at its next clean point and exits." : before;

const snapshotSize = (view: ScenarioView): string => {
  const snapshot = latestSnapshot(view.root);
  return snapshot ? formatBytes(snapshot.bytes) : "small";
};

export const SCENARIOS: readonly Scenario[] = [
  {
    id: "migrate",
    title: "Pause here, resume there",
    summary:
      "A trip planner runs in a loop on the local site. You pause it in the middle. Its process ends, and the whole run becomes a snapshot file of a few kilobytes. You resume it on a cloud site, where a new process continues the loop at the same line with the same variables.",
    shows: ["pause", "snapshot", "migration", "remote call"],
    start: { site: "local", fn: "durable_plan_trip", args: CITY },
    fixture: "pool",
    steps: [
      {
        id: "pause",
        title: "pause",
        action: { kind: "command", role: "root", action: "pause" },
        ready: (view) => statusOf(view.root) === "running" && planned(view.root) >= 1,
        waitHint: (view) => pauseWait(view, "The run starts on local. Wait for its first loop iteration."),
        hint: () => "Click Pause now. The run is inside its loop on local.",
        done: (view) => paused(view.root),
      },
      {
        id: "move",
        title: "resume on cloud",
        action: { kind: "command", role: "root", action: "resume_on", site: "cloud" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The worker writes the snapshot at its next clean point and exits.",
        hint: (view) => `No process exists. The whole run is a ${snapshotSize(view)} snapshot. Click Resume on cloud.`,
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "finish",
        title: "continue",
        hint: (view) =>
          `A new process on ${view.root?.current?.site ?? "cloud"} continues the loop where the old one stopped. Its remote call runs on ${sitesOf(view.root?.children ?? [])}.`,
        done: (view) => completed(view.root),
      },
    ],
    outcome: (view) => `Done. The result came from a process on ${view.root?.current?.site ?? "cloud"} that never saw the first half of the run.`,
  },
  {
    id: "fanout",
    title: "Fan-out with a durable sleep",
    summary:
      "The run asks four vendors (flight, hotel, car, tour) for a quote at the same time. Each request is a remote call that runs on a cloud machine. The run then sleeps for 12 seconds. It does not hold a process for that: it suspends itself, and a timer wakes it. The four children finish while no parent process exists, and every result is there at the resume.",
    shows: ["spawn", "4 remote children", "durable sleep", "future.all", "classes and maps"],
    start: { site: "local", fn: "durable_fan_out", args: CITY },
    fixture: "fanout",
    steps: [
      {
        id: "spawn",
        title: "spawn four",
        hint: (view) =>
          `The run spawns four threads. Each calls remote_get_quote with a QuoteRequest, and the pool alternates between cloud and cloud2. ${countWord(view.root?.children.length ?? 0)} of four started.`,
        done: (view) => (view.root?.children.length ?? 0) >= 4,
      },
      {
        id: "suspend",
        title: "suspend itself",
        hint: () => "The parent reaches sleep(12 s). Every thread is parked in a wait that can be issued again, so the run suspends itself: snapshot, exit, wake timer.",
        done: (view) => eventsOf(view.root, "paused").some((event) => event.wake != null) || eventsOf(view.root, "sleep_scheduled").length > 0 || statusOf(view.root) === "sleeping",
      },
      {
        id: "sleep",
        title: "sleep, no process",
        hint: (view) => {
          const total = view.root?.children.length ?? 4;
          const finished = childrenWith(view.root, "completed", "failed").length;
          const wakeAt = view.root?.current?.wake_at;
          const countdown = typeof wakeAt === "number" ? ` The timer wakes the run in ${formatCountdown(wakeAt - view.now)}.` : "";
          return `Watch: no parent process exists while the run sleeps. ${finished} of ${total} children have finished on their machines, and their results are stored.${countdown}`;
        },
        done: (view) => eventsOf(view.root, "woken").length > 0 || segmentsStarted(view.root) >= 2,
      },
      {
        id: "collect",
        title: "wake and collect",
        hint: () => "The timer woke the run. A new process restores five threads, gets the stored results at startup, and baml.future.all returns the quotes.",
        done: (view) => completed(view.root),
      },
    ],
    outcome: () => "Done. Four results crossed a gap in which the run held no process and no memory. The report holds nested classes, enums, arrays, and maps.",
  },
  {
    id: "race",
    title: "Race and cancellation",
    summary:
      "Three hotel vendors race: baml.future.race takes the first answer and cancels the rest. The losers are separate processes on cloud machines, and the cancellation reaches them there. You can pause the parent in the middle of the race and resume it on another site. The winner's result follows the run, and the losers are cancelled from its new site.",
    shows: ["future.race", "pause in a race", "migration", "remote cancel"],
    start: { site: "local", fn: "durable_race", args: CITY },
    fixture: "race",
    steps: [
      {
        id: "spawn",
        title: "start three",
        hint: (view) => `Three remote calls of different length start on cloud and cloud2. ${countWord(view.root?.children.length ?? 0)} of three started.`,
        done: (view) => (view.root?.children.length ?? 0) >= 3,
      },
      {
        id: "pause",
        title: "pause mid-race",
        optional: true,
        action: { kind: "command", role: "root", action: "pause" },
        ready: (view) => statusOf(view.root) === "running" && runningChildren(view.root).length >= 2,
        waitHint: (view) => pauseWait(view, "The children are starting."),
        hint: (view) => {
          const running = runningChildren(view.root);
          return `Click Pause now: ${countWord(running.length)} children are running on ${sitesOf(running)}. The race survives the pause.`;
        },
        done: (view) => paused(view.root),
      },
      {
        id: "resume",
        title: "resume on cloud2",
        action: { kind: "command", role: "root", action: "resume_on", site: "cloud2" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The worker writes five threads and four pending futures into the snapshot.",
        hint: (view) =>
          childrenWith(view.root, "completed").length > 0
            ? "The fastest vendor answered while no parent process existed, and its result is stored. Click Resume on cloud2."
            : "The snapshot holds five threads and four pending futures. The children keep running. Click Resume on cloud2.",
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "cancel",
        title: "cancel the losers",
        hint: () => "race settles with the first result and cancels the other futures. The worker reports a remote_cancel for each loser.",
        done: (view) => cancels(view.root) >= 1,
      },
      {
        id: "losers",
        title: "losers end",
        hint: (view) => `The site servers end the losers on their machines: ${childrenWith(view.root, "cancelled").length} cancelled so far.`,
        done: (view) => completed(view.root) && runningChildren(view.root).length === 0,
      },
    ],
    outcome: (view) => `Done. One winner and ${countWord(childrenWith(view.root, "cancelled").length)} cancelled processes. No work is left running on the pool.`,
  },
  {
    id: "deadline",
    title: "Deadline",
    summary:
      "A tour vendor needs 6 seconds, and the run gives it 2. baml.future.with_timeout runs the call under a cancel token next to a deadline thread that sleeps. When the deadline passes, the token fires, the waiting thread is cancelled, and the cloud site ends the child's process. The deadline keeps counting while the run is paused.",
    shows: ["with_timeout", "cancel token", "remote cancel"],
    start: { site: "local", fn: "durable_deadline", args: CITY },
    fixture: "deadline",
    steps: [
      {
        id: "call",
        title: "slow call",
        hint: () => "A work thread calls the slow vendor on cloud. A deadline thread sleeps for 2 seconds.",
        done: (view) => (view.root?.children.length ?? 0) >= 1,
      },
      {
        id: "pause",
        title: "pause",
        optional: true,
        action: { kind: "command", role: "root", action: "pause" },
        ready: (view) => statusOf(view.root) === "running" && runningChildren(view.root).length >= 1 && cancels(view.root) === 0,
        waitHint: (view) => pauseWait(view, "The child is starting on cloud."),
        hint: () => "Optional: click Pause. The state tree then shows the cancel token, the work thread, and the deadline thread.",
        done: (view) => paused(view.root),
      },
      {
        id: "resume",
        title: "resume",
        action: { kind: "command", role: "root", action: "resume_here" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The worker writes three threads, two pending futures, and the cancel token into the snapshot.",
        hint: () => "The deadline keeps counting while no process exists. Click Resume here. A sleep whose deadline has passed completes at once.",
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "timeout",
        title: "deadline passes",
        hint: () => "The deadline passes: the token fires and cancels the work thread in its remote wait. The worker reports remote_cancel.",
        done: (view) => cancels(view.root) >= 1,
      },
      {
        id: "child",
        title: "child cancelled",
        hint: () => "The cloud site ends the child's process. The function builds its result from the Timeout error.",
        done: (view) => completed(view.root),
      },
    ],
    outcome: (view) => {
      const result = view.root?.current?.result;
      return `Done. ${typeof result === "string" ? `Result: "${result}". ` : ""}The 6 second job was cancelled on its machine.`;
    },
  },
  {
    id: "settled",
    title: "All settled, one failure",
    summary:
      "Three vendors are asked, and the car vendor refuses with a typed error. baml.future.all_settled waits for every vendor and reports one outcome per vendor. Pause the run after the refusal: the state tree then shows futures in three states, one resolved with a class instance, one failed with the error, and one still pending.",
    shows: ["future.all_settled", "typed error", "futures in the state tree"],
    start: { site: "local", fn: "durable_settled", args: CITY },
    fixture: "settled",
    steps: [
      {
        id: "spawn",
        title: "start three",
        hint: (view) => `Three remote calls start. ${countWord(view.root?.children.length ?? 0)} of three started.`,
        done: (view) => (view.root?.children.length ?? 0) >= 3,
      },
      {
        id: "first",
        title: "one answers",
        hint: () => "The flight vendor answers first. The car vendor is about to refuse with QuoteUnavailable. all_settled cancels nothing and waits for every vendor.",
        done: (view) => childrenWith(view.root, "completed", "failed").length >= 1,
      },
      {
        id: "pause",
        title: "pause",
        optional: true,
        action: { kind: "command", role: "root", action: "pause" },
        ready: (view) => statusOf(view.root) === "running" && runningChildren(view.root).length >= 1,
        waitHint: (view) => pauseWait(view, "Wait for the first answer."),
        hint: (view) =>
          childrenWith(view.root, "failed").length > 0
            ? "Click Pause now: one vendor answered, one refused, and one is still out. The snapshot will hold a future in each state."
            : "Click Pause now, or wait a moment for the refusal. The snapshot holds every future as it is: resolved, failed, or pending.",
        done: (view) => paused(view.root),
      },
      {
        id: "resume",
        title: "inspect, resume",
        action: { kind: "command", role: "root", action: "resume_here" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The worker writes the snapshot.",
        hint: () => "Open the futures in the state tree: resolved with a Quote, failed with the vendor's error, and pending. Then click Resume here.",
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "report",
        title: "report",
        hint: () => "The last result settles the last future, and the run returns its report.",
        done: (view) => completed(view.root),
      },
    ],
    outcome: () => "Done. One outcome per vendor: two quotes and one refusal. The refusal cancelled nothing.",
  },
  {
    id: "recover",
    title: "Kill and recover",
    summary:
      "The same function runs twice, once as a durable function and once as a plain one. You kill the process of each. The durable run wrote a snapshot after every loop iteration, so it is paused, and a new process continues it. The plain run is lost.",
    shows: ["automatic snapshots", "kill", "durable and plain"],
    start: { site: "local", fn: "durable_plan_trip", args: CITY },
    fixture: "recover",
    steps: [
      {
        id: "snapshot",
        title: "auto snapshot",
        hint: () => "The durable run writes an automatic snapshot after every loop iteration. Wait for the first one.",
        done: (view) => eventsOf(view.root, "snapshot").length >= 1 || (view.root?.current?.snapshots.length ?? 0) >= 1,
      },
      {
        id: "kill",
        title: "kill",
        action: { kind: "command", role: "root", action: "kill" },
        ready: (view) => statusOf(view.root) === "running",
        hint: () => "Kill the process now. It gets no chance to save anything.",
        done: (view) => lostProcess(view.root),
      },
      {
        id: "resume",
        title: "resume",
        action: { kind: "command", role: "root", action: "resume_here" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The site server notices that the process is gone.",
        hint: (view) => `The run is paused, not lost: snapshot #${latestSnapshot(view.root)?.n ?? 1} survived the kill. Click Resume here.`,
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "finish",
        title: "complete",
        hint: () => "A new process repeats the interrupted day from the snapshot, makes the remote call, and completes.",
        done: (view) => completed(view.root),
      },
      {
        id: "plain",
        title: "start plain",
        action: { kind: "start", as: "plain", start: { site: "local", fn: "plan_trip", args: CITY } },
        hint: () => "Now the same body without durability. Start plan_trip.",
        done: (view) => (view.plain?.records.length ?? 0) > 0,
      },
      {
        id: "kill-plain",
        title: "kill plain",
        action: { kind: "command", role: "plain", action: "kill" },
        ready: (view) => statusOf(view.plain) === "running" && planned(view.plain) >= 1,
        waitHint: () => "plan_trip starts its loop.",
        hint: () => "Kill this process too.",
        done: (view) => isTerminal(view.plain),
      },
    ],
    outcome: (view) =>
      statusOf(view.plain) === "lost"
        ? "Done. The durable run survived the kill and completed. The plain run is lost: it had nothing to continue from."
        : "Done. The durable run survived the kill and completed.",
  },
  {
    id: "fork",
    title: "Fork a run",
    summary:
      "A snapshot is a value, so it can be copied. You pause a run, fork it, and resume both. Two processes continue from the same state, each makes its own remote call, and both complete with the same result.",
    shows: ["pause", "fork", "two runs from one snapshot"],
    start: { site: "local", fn: "durable_plan_trip", args: CITY },
    fixture: "branch",
    steps: [
      {
        id: "pause",
        title: "pause",
        action: { kind: "command", role: "root", action: "pause" },
        ready: (view) => statusOf(view.root) === "running" && planned(view.root) >= 2,
        waitHint: (view) => pauseWait(view, "Wait until the run plans day 2."),
        hint: () => "Click Pause during day 2.",
        done: (view) => paused(view.root),
      },
      {
        id: "fork",
        title: "fork",
        action: { kind: "command", role: "root", action: "fork" },
        ready: (view) => statusOf(view.root) === "paused",
        waitHint: () => "The worker writes the snapshot.",
        hint: () => "Click Fork. The new run gets its own copy of the snapshot.",
        done: (view) => view.fork !== null,
      },
      {
        id: "resume-source",
        title: "resume source",
        action: { kind: "command", role: "root", action: "resume_here" },
        ready: (view) => statusOf(view.root) === "paused",
        hint: () => "Click Resume here to continue the source run.",
        done: (view) => segmentsStarted(view.root) >= 2,
      },
      {
        id: "resume-fork",
        title: "resume fork",
        action: { kind: "command", role: "fork", action: "resume_here" },
        ready: (view) => statusOf(view.fork) === "paused",
        hint: () => "Now resume the fork. It continues from the same state in a process of its own.",
        done: (view) => segmentsStarted(view.fork) >= 1,
      },
      {
        id: "both",
        title: "both complete",
        hint: () => "Two runs continue from one snapshot. Each plans day 3 and makes its own remote call.",
        done: (view) => completed(view.root) && isTerminal(view.fork),
      },
    ],
    outcome: () => "Done. Both runs completed with the same result.",
  },
];

export function scenarioById(id: string | null | undefined): Scenario | null {
  return SCENARIOS.find((scenario) => scenario.id === id) ?? null;
}

/** The scenario that a fixture plays, or `null` for a fixture that no scenario uses. */
export function scenarioOfFixture(fixture: string | null | undefined): Scenario | null {
  return SCENARIOS.find((scenario) => scenario.fixture === fixture) ?? null;
}
