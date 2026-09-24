/**
 * Fixture "settled": `baml.future.all_settled` over three remote calls of
 * which one throws a typed error (contract sections 9.1 and 9.4).
 *
 * `durable_settled` asks three vendors. The flight vendor answers after 2
 * seconds, and the car vendor throws `QuoteUnavailable` after 3 seconds.
 * `all_settled` keeps waiting for the tour vendor. The user pauses the run at
 * that moment, so the snapshot holds futures in three states: resolved with a
 * class instance, failed with an error, and pending. The tour vendor answers
 * while no parent process exists. The user resumes the run, and it returns one
 * outcome per vendor.
 */

import type { DumpThread } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineIn } from "./programs";
import { QUOTES_BAML_FILE } from "./quotes.baml";
import {
  completeQuoteChild,
  dv,
  failQuoteChild,
  frame,
  heapOf,
  println,
  quoteDump,
  quoteValue,
  refusalOf,
  remoteWaitThread,
  requestDump,
  resumeProcess,
  snapshotStats,
  spawnQuoteCall,
  startProcess,
  startQuoteChild,
  type Caller,
  type ChildSpec,
  type Vendor,
} from "./quotes";

export const SETTLED_PARENT = "r-st9b4m";
export const SETTLED_CHILDREN = ["r-qj3f7x", "r-qk6d2y", "r-ql1g8z"] as const;

const VENDORS: readonly Vendor[] = [
  { kind: "Flight", delayMs: 2000 },
  { kind: "Car", delayMs: 3000, refuses: true },
  { kind: "Tour", delayMs: 4000 },
];
const CHILDREN: readonly ChildSpec[] = [
  { site: "cloud", run: SETTLED_CHILDREN[0], pid: 52_602 },
  { site: "cloud2", run: SETTLED_CHILDREN[1], pid: 61_230 },
  { site: "cloud", run: SETTLED_CHILDREN[2], pid: 52_619 },
];
const COLLECTOR = 5;

export function buildSettledFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_settled";
  const city = "Lisbon";
  const parent = SETTLED_PARENT;
  const caller: Caller = { site: "local", run: parent, fn };
  const callId = (index: number): string => `${parent}-c${index + 1}`;
  const flight = VENDORS[0] as Vendor;
  const car = VENDORS[1] as Vendor;
  const tour = VENDORS[2] as Vendor;
  // The child's typed error crosses the sites as text. The parent receives it as `baml.errors.Io`.
  const failure = `QuoteUnavailable { kind: Car, city: "${city}", reason: "${refusalOf(city, car)}" }`;

  b.createRun(0, "local", parent, fn, { city });
  startProcess(b, 34, "local", parent, fn, 41_840, true);
  println(b, 40, caller, "asking three vendors", `asking three vendors for ${city}, one of them will refuse`);
  const childDone = VENDORS.map((vendor, index) => {
    const at = 50 + index * 14;
    spawnQuoteCall(b, at, caller, index + 2, callId(index), city, vendor);
    return startQuoteChild(b, at + 9, caller, callId(index), CHILDREN[index] as ChildSpec, city, vendor);
  });
  // `baml.future.all_settled` collects on a thread of its own, spawned at the await line.
  b.worker(96, "local", parent, { type: "thread_started", thread: COLLECTOR, parent_thread: 1, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "all_settled") });
  b.worker(98, "local", parent, { type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, fn, "all_settled"), reason: "await", op: null });

  // The flight vendor answers, and the car vendor throws. Both results reach the running process.
  let t = completeQuoteChild(b, childDone[0] as number, caller, callId(0), CHILDREN[0] as ChildSpec, city, flight, true);
  b.worker(t + 3, "local", parent, { type: "remote_result_received", call_id: callId(0), thread: 2 });
  b.worker(t + 5, "local", parent, { type: "thread_ended", thread: 2 });
  t = failQuoteChild(b, childDone[1] as number, caller, callId(1), CHILDREN[1] as ChildSpec, failure, true);
  b.worker(t + 3, "local", parent, { type: "remote_result_received", call_id: callId(1), thread: 3 });
  b.worker(t + 5, "local", parent, { type: "thread_ended", thread: 3 });

  // The user pauses the run while the tour vendor is still out.
  const pauseAt = 3400;
  b.setStatus(pauseAt, "local", parent, "pausing");
  const remoteError = dv.instance("baml.errors.Io", { message: dv.str(failure) });
  const main: DumpThread = {
    thread: 1,
    name: "main",
    parked: { kind: "await", detail: "await on the future of baml.future.all_settled (thread 5)" },
    frames: [
      frame(fn, "all_settled", [
        ["city", "string", dv.str(city)],
        ["car_request", "QuoteRequest", requestDump(city, car)],
        ["kinds", "QuoteKind[]", dv.array("QuoteKind", [dv.variant("QuoteKind", "Flight"), dv.variant("QuoteKind", "Car"), dv.variant("QuoteKind", "Tour")])],
        ["flight", "Future<Quote>", dv.future(2, "ready", quoteDump(city, flight))],
        ["car", "Future<Quote>", dv.future(3, "failed", remoteError)],
        ["tour", "Future<Quote>", dv.future(4, "pending")],
      ]),
    ],
  };
  const collector: DumpThread = {
    thread: COLLECTOR,
    parent_thread: 1,
    settles_future: COLLECTOR,
    name: "baml.future.all_settled",
    parked: { kind: "await", detail: "await on future #4 (tour)" },
    frames: [
      {
        function: "baml.future.all_settled.<spawn>",
        file: "<builtin>/baml/ns_future/future.baml",
        line: 104,
        locals: [
          {
            name: "outcomes",
            type: "(Success<Quote> | Failure<QuoteUnavailable | Io> | Panicked)[]",
            value: dv.array("Success<Quote> | Failure<QuoteUnavailable | Io> | Panicked", [
              dv.instance("baml.future.Success<Quote>", { value: quoteDump(city, flight) }),
              dv.instance("baml.future.Failure<QuoteUnavailable | Io>", { error: remoteError }),
            ]),
          },
          { name: "f", type: "Future<Quote>", value: dv.future(4, "pending") },
        ],
      },
    ],
  };
  b.snapshot(pauseAt + 11, "local", parent, 1, snapshotStats(11, 97, 16_380), {
    threads: [main, remoteWaitThread(4, fn, city, tour, callId(2)), collector],
    heap: heapOf(97, 16_380),
  });

  // The tour vendor answers while the parent has no process.
  completeQuoteChild(b, childDone[2] as number, caller, callId(2), CHILDREN[2] as ChildSpec, city, tour, false);

  // The user resumes the run. The stored result settles the last future.
  const resumeAt = 5600;
  b.setStatus(resumeAt, "local", parent, "starting", { segment: 2 });
  resumeProcess(b, resumeAt + 38, "local", parent, fn, 41_871);
  t = resumeAt + 48;
  b.worker(t, "local", parent, { type: "remote_result_received", call_id: callId(2), thread: 4 });
  b.worker(t + 2, "local", parent, { type: "thread_ended", thread: 4 });
  b.update(t + 2, "local", parent, { remote_results: b.run("local", parent).remote_results.map((result) => ({ ...result, acked: true })) });
  b.worker(t + 5, "local", parent, { type: "thread_ended", thread: COLLECTOR });
  println(b, t + 8, caller, "report.succeeded.length()", "2 quotes, 1 refused");
  const value = { city, succeeded: [quoteValue(city, flight), quoteValue(city, tour)], failed: [{ kind: "Car", error: failure }] };
  b.worker(t + 11, "local", parent, { type: "thread_ended", thread: 1 });
  b.worker(t + 12, "local", parent, { type: "completed", value });
  b.exit(t + 30, "local", parent, 0, "completed", { result: value });

  return b.build("settled", "all_settled over three remote calls, one of which throws", { roles: { root: { site: "local", id: parent } } });
}
