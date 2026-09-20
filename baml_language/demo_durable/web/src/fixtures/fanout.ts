/**
 * Fixture "fanout": a fan-out with a durable sleep (contract sections 9.2 to 9.4).
 *
 * `durable_fan_out` starts on the local site and spawns four threads. Each
 * calls `remote_get_quote`, and the pool places the children on cloud and
 * cloud2 in turn. The parent then reaches `sleep(12 s)`. Every thread is parked
 * in a wait that can be issued again, so the run suspends itself: it writes a
 * snapshot with five threads, the process exits, and the site server sets a
 * wake timer. The four children finish while no parent process exists, and the
 * local site stores their results. The timer resumes the run. The new process
 * gets the four results at startup, the threads settle, and
 * `baml.future.all` returns the quotes.
 *
 * The resumed worker announces only the root thread. The spawned threads come
 * back without a `thread_started` event, which is the case that the timeline
 * has to carry over by itself. `buildFanoutFixture(8)` makes eight calls, for
 * the row stacking tests.
 */

import type { DumpThread, Site } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineIn } from "./programs";
import { QUOTES_BAML_FILE } from "./quotes.baml";
import {
  completeQuoteChild,
  dv,
  frame,
  heapOf,
  println,
  quoteAmount,
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
  vendorName,
} from "./quotes";

export const FANOUT_PARENT = "r-fn7q2a";
export const FANOUT_CHILDREN = ["r-qh4m1c", "r-qa8t5d", "r-qm2x9e", "r-qz6k3f", "r-qb1v7g", "r-qd5n2h", "r-qe9p4j", "r-qg3r8k"] as const;

export const FANOUT_VENDORS: readonly Vendor[] = [
  { kind: "Flight", delayMs: 3000 },
  { kind: "Hotel", delayMs: 5000 },
  { kind: "Car", delayMs: 2000 },
  { kind: "Tour", delayMs: 4000 },
];
/** Four more calls, for the tests that stack eight children. The program of the demo makes the first four. */
const EXTRA_VENDORS: readonly Vendor[] = [
  { kind: "Flight", delayMs: 3600 },
  { kind: "Hotel", delayMs: 5600 },
  { kind: "Car", delayMs: 2600 },
  { kind: "Tour", delayMs: 4600 },
];
const LOCALS = ["flight", "hotel", "car", "tour"];

const POOL: readonly Site[] = ["cloud", "cloud2"];
const SLEEP_MS = 12_000;

export function buildFanoutFixture(width = 4): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_fan_out";
  const city = "Lisbon";
  const parent = FANOUT_PARENT;
  const caller: Caller = { site: "local", run: parent, fn };
  const vendors = [...FANOUT_VENDORS, ...EXTRA_VENDORS].slice(0, width);
  // An extra call has no line of its own in the program. It borrows the line of the call of its kind.
  const onLine = (vendor: Vendor): Vendor => FANOUT_VENDORS.find((candidate) => candidate.kind === vendor.kind) ?? vendor;
  const callId = (index: number): string => `${parent}-c${index + 1}`;
  const child = (index: number): ChildSpec => ({
    // The caller's site places the children in round-robin order (section 9.3).
    site: POOL[index % POOL.length] as Site,
    run: FANOUT_CHILDREN[index] as string,
    pid: 52_300 + index * 17,
  });

  // Segment 1: the parent spawns the calls and reaches the sleep.
  b.createRun(0, "local", parent, fn, { city });
  startProcess(b, 34, "local", parent, fn, 41_620, true);
  println(b, 40, caller, "asking four vendors", `asking four vendors for ${city}`);
  const childDone: number[] = [];
  vendors.forEach((vendor, index) => {
    const at = 50 + index * 14;
    spawnQuoteCall(b, at, caller, index + 2, callId(index), city, vendor, onLine(vendor));
    childDone.push(startQuoteChild(b, at + 9, caller, callId(index), child(index), city, vendor));
  });
  const sleepAt = 62 + vendors.length * 14;
  println(b, sleepAt, caller, "sleeping 12 seconds", "sleeping 12 seconds while the vendors work");
  b.worker(sleepAt + 3, "local", parent, {
    type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep",
  });

  // The run suspends itself. Five threads go into the snapshot.
  const suspendAt = sleepAt + 19;
  const remaining = SLEEP_MS - 16;
  const main: DumpThread = {
    thread: 1,
    name: "main",
    parked: { kind: "sleep", detail: `baml.sys.sleep, ${remaining} ms remain; the sleep is issued again with the rest at the resume` },
    frames: [
      frame(fn, "baml.sys.sleep", [
        ["city", "string", dv.str(city)],
        ...vendors.map((vendor, index): [string, string, ReturnType<typeof dv.future>] => [LOCALS[index] ?? `${vendor.kind.toLowerCase()}_${index}`, "Future<Quote>", dv.future(index + 2, "pending")]),
      ]),
    ],
  };
  const objects = 46 + vendors.length * 14;
  const wakeAt = b.selfSuspend(suspendAt, "local", parent, 1, snapshotStats(null, objects, 9800 + vendors.length * 1450), remaining, {
    threads: [main, ...vendors.map((vendor, index) => remoteWaitThread(index + 2, fn, city, { ...vendor, delayMs: onLine(vendor).delayMs }, callId(index)))],
    heap: heapOf(objects, 9800 + vendors.length * 1450),
  });

  // The children finish while the parent has no process. The local site stores the results.
  // `completeQuoteChild` reads the record of the parent, so the children are ended in the order of their ends.
  vendors
    .map((vendor, index) => ({ vendor, index, at: childDone[index] as number }))
    .sort((a, c) => a.at - c.at)
    .forEach(({ vendor, index, at }) => completeQuoteChild(b, at, caller, callId(index), child(index), city, vendor, false));

  // The wake timer fires. The new process gets the results at startup.
  b.wake(wakeAt, "local", parent, "timer");
  resumeProcess(b, wakeAt + 31, "local", parent, fn, 41_884);
  let t = wakeAt + 40;
  vendors.forEach((_, index) => {
    b.worker(t, "local", parent, { type: "remote_result_received", call_id: callId(index), thread: index + 2 });
    b.worker(t + 2, "local", parent, { type: "thread_ended", thread: index + 2 });
    t += 3;
  });
  b.update(t, "local", parent, { remote_results: b.run("local", parent).remote_results.map((result) => ({ ...result, acked: true })) });
  println(b, t + 2, caller, "awake again", "awake again, collecting the quotes");
  // `baml.future.all` awaits the inputs on a thread of its own.
  const collector = vendors.length + 2;
  b.worker(t + 6, "local", parent, { type: "thread_started", thread: collector, parent_thread: 1, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "baml.future.all") });
  b.worker(t + 7, "local", parent, { type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "baml.future.all"), reason: "await", op: null });
  b.worker(t + 11, "local", parent, { type: "thread_ended", thread: collector });
  b.worker(t + 13, "local", parent, { type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "TripReport {"), reason: "early_yield", op: null });
  const quotes = vendors.map((vendor) => quoteValue(city, vendor));
  const cheapest = vendors.reduce((best, vendor) => (quoteAmount(vendor) < quoteAmount(best) ? vendor : best), vendors[0] as Vendor);
  const value = {
    city,
    quotes,
    total: { amount: vendors.reduce((sum, vendor) => sum + quoteAmount(vendor), 0), currency: "EUR" },
    vendors: Object.fromEntries(vendors.map((vendor) => [vendor.kind, vendorName(vendor)])),
    cheapest: quoteValue(city, cheapest),
  };
  b.worker(t + 16, "local", parent, { type: "thread_ended", thread: 1 });
  b.worker(t + 17, "local", parent, { type: "completed", value });
  b.exit(t + 20, "local", parent, 0, "completed", { result: value });

  return b.build("fanout", "A fan-out of four remote calls and a durable sleep", { roles: { root: { site: "local", id: parent } } });
}
