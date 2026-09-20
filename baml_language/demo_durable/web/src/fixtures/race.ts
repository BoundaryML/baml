/**
 * Fixture "race": `baml.future.race` over three remote calls, a pause in the
 * middle of the race, and the cancellation of the losers (contract sections
 * 9.1 to 9.4).
 *
 * `durable_race` starts on the local site and spawns three threads. The pool
 * places the fast vendor (2 s) on cloud, the medium one (6 s) on cloud2, and
 * the slow one (9 s) on cloud. `race` waits on a thread of its own. The user pauses the run
 * while all three children run: five threads and four pending futures go into
 * the snapshot. The fast vendor answers while no parent process exists, and the local
 * site stores the result. The user resumes the run on cloud2. The result
 * travels with the run, `race` settles with it, and it cancels the two
 * losers: the worker reports `remote_cancel` twice, and the site servers end
 * the children on cloud2 and on cloud.
 *
 * The resumed worker announces every restored thread with `thread_started`.
 */

import type { DumpThread, DumpValue } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineIn } from "./programs";
import { QUOTES_BAML_FILE } from "./quotes.baml";
import {
  callNeedle,
  cancelQuoteChild,
  completeQuoteChild,
  dv,
  frame,
  heapOf,
  println,
  quoteValue,
  remoteWaitThread,
  resumeProcess,
  snapshotStats,
  spawnQuoteCall,
  startProcess,
  startQuoteChild,
  type Caller,
  type ChildSpec,
  type Vendor,
} from "./quotes";

export const RACE_PARENT = "r-rc3w8n";
export const RACE_CHILDREN = ["r-qw2h6p", "r-qs7a1r", "r-qu4z9s"] as const;

const VENDORS: readonly Vendor[] = [
  { kind: "Hotel", delayMs: 2000 },
  { kind: "Hotel", delayMs: 6000 },
  { kind: "Hotel", delayMs: 9000 },
];
const CHILDREN: readonly ChildSpec[] = [
  { site: "cloud", run: RACE_CHILDREN[0], pid: 52_410 },
  { site: "cloud2", run: RACE_CHILDREN[1], pid: 61_122 },
  { site: "cloud", run: RACE_CHILDREN[2], pid: 52_431 },
];
/** The thread on which `baml.future.race` waits for the first input. */
const RACE_THREAD = 5;

export function buildRaceFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_race";
  const city = "Lisbon";
  const parent = RACE_PARENT;
  const onLocal: Caller = { site: "local", run: parent, fn };
  const callId = (index: number): string => `${parent}-c${index + 1}`;
  const raceLine = lineIn(QUOTES_BAML_FILE, fn, "baml.future.race");

  b.createRun(0, "local", parent, fn, { city });
  startProcess(b, 34, "local", parent, fn, 41_705, true);
  println(b, 40, onLocal, "racing three hotel vendors", `racing three hotel vendors for ${city}`);
  const childDone = VENDORS.map((vendor, index) => {
    const at = 50 + index * 14;
    spawnQuoteCall(b, at, onLocal, index + 2, callId(index), city, vendor);
    return startQuoteChild(b, at + 9, onLocal, callId(index), CHILDREN[index] as ChildSpec, city, vendor);
  });
  // `baml.future.race` waits on a thread of its own, spawned at the await line.
  b.worker(96, "local", parent, { type: "thread_started", thread: RACE_THREAD, parent_thread: 1, file: QUOTES_BAML_FILE, line: raceLine });
  b.worker(98, "local", parent, { type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: raceLine, reason: "await", op: null });

  // The user pauses the run while the three children run.
  const pauseAt = 1300;
  b.setStatus(pauseAt, "local", parent, "pausing");
  const pending = (thread: number): DumpValue => dv.future(thread, "pending");
  const main: DumpThread = {
    thread: 1,
    name: "main",
    parked: { kind: "await", detail: "await on the future of baml.future.race (thread 5)" },
    frames: [
      frame(fn, "baml.future.race", [
        ["city", "string", dv.str(city)],
        ["fast", "Future<Quote>", pending(2)],
        ["medium", "Future<Quote>", pending(3)],
        ["slow", "Future<Quote>", pending(4)],
      ]),
    ],
  };
  const racer: DumpThread = {
    thread: RACE_THREAD,
    parent_thread: 1,
    settles_future: RACE_THREAD,
    name: "baml.future.race",
    parked: { kind: "await_any", detail: "__await_any over 3 futures; none has settled" },
    frames: [
      {
        function: "baml.future.race.<spawn>",
        file: "<builtin>/baml/ns_future/future.baml",
        line: 186,
        locals: [{ name: "futures", type: "Future<Quote>[]", value: dv.array("Future<Quote>", [pending(2), pending(3), pending(4)]) }],
      },
    ],
  };
  b.snapshot(pauseAt + 12, "local", parent, 1, snapshotStats(12, 88, 14_620), {
    threads: [main, ...VENDORS.map((vendor, index) => remoteWaitThread(index + 2, fn, city, vendor, callId(index))), racer],
    heap: heapOf(88, 14_620),
  });

  // The fastest child finishes while the parent has no process.
  completeQuoteChild(b, childDone[0] as number, onLocal, callId(0), CHILDREN[0] as ChildSpec, city, VENDORS[0] as Vendor, false);

  // The user resumes the run on cloud2. The stored result travels with it.
  const migrateAt = 3400;
  b.migrate(migrateAt, "local", "cloud2", parent);
  const onCloud2: Caller = { site: "cloud2", run: parent, fn };
  resumeProcess(b, migrateAt + 71, "cloud2", parent, fn, 61_140);
  let t = migrateAt + 78;
  // A restored thread is announced again, with the `spawn` site it was created at.
  const spawnLineOf = (thread: number): number =>
    thread === RACE_THREAD ? raceLine : lineIn(QUOTES_BAML_FILE, fn, callNeedle(VENDORS[thread - 2] as Vendor));
  for (const thread of [2, 3, 4, RACE_THREAD]) {
    b.worker(t, "cloud2", parent, { type: "thread_started", thread, parent_thread: 1, file: QUOTES_BAML_FILE, line: spawnLineOf(thread) });
  }
  b.worker(t + 3, "cloud2", parent, { type: "remote_result_received", call_id: callId(0), thread: 2 });
  b.worker(t + 5, "cloud2", parent, { type: "thread_ended", thread: 2 });
  b.update(t + 5, "cloud2", parent, { remote_results: b.run("cloud2", parent).remote_results.map((result) => ({ ...result, acked: true })) });
  // `race` settles with the winner and cancels the losers.
  t += 9;
  cancelQuoteChild(b, t, onCloud2, callId(1), 3, CHILDREN[1] as ChildSpec, "future_cancel");
  b.worker(t + 1, "cloud2", parent, { type: "thread_ended", thread: 3 });
  cancelQuoteChild(b, t + 4, onCloud2, callId(2), 4, CHILDREN[2] as ChildSpec, "future_cancel");
  b.worker(t + 5, "cloud2", parent, { type: "thread_ended", thread: 4 });
  b.worker(t + 9, "cloud2", parent, { type: "thread_ended", thread: RACE_THREAD });
  println(b, t + 12, onCloud2, "the winner answered", "the winner answered after 2000 ms");
  const value = quoteValue(city, VENDORS[0] as Vendor);
  b.worker(t + 16, "cloud2", parent, { type: "thread_ended", thread: 1 });
  b.worker(t + 17, "cloud2", parent, { type: "completed", value });
  b.exit(t + 40, "cloud2", parent, 0, "completed", { result: value });

  return b.build("race", "A race over three remote calls, paused in the middle, with two cancelled losers", { roles: { root: { site: "local", id: parent } } });
}
