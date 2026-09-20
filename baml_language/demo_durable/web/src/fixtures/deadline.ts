/**
 * Fixture "deadline": `baml.future.with_timeout` cancels a remote child
 * (contract sections 9.1 to 9.4).
 *
 * `durable_deadline` gives a vendor that needs 6 seconds a deadline of 2
 * seconds. `with_timeout` runs the body on a work thread under a cancel token
 * and starts a deadline thread that sleeps. The user pauses the run after 0.9
 * seconds: three threads, two pending futures, and the cancel token go into
 * the snapshot. The user resumes the run in a new process. The rest of the
 * deadline passes there, the token fires, the work thread is cancelled
 * (`remote_cancel`), the cloud site ends the child, and the function returns a
 * message built from the `Timeout` error.
 */

import type { DumpThread } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineIn } from "./programs";
import { QUOTES_BAML_FILE } from "./quotes.baml";
import { callNeedle, cancelQuoteChild, dv, frame, heapOf, println, quoteRequest, REMOTE_QUOTE_FN, remoteWaitThread, resumeProcess, snapshotStats, startProcess, startQuoteChild, type Caller, type ChildSpec, type Vendor } from "./quotes";

export const DEADLINE_PARENT = "r-dl5k2v";
export const DEADLINE_CHILD = "r-qg8c3w";

const VENDOR: Vendor = { kind: "Tour", delayMs: 6000 };
const WORK_THREAD = 2;
const DEADLINE_THREAD = 3;

export function buildDeadlineFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_deadline";
  const city = "Lisbon";
  const parent = DEADLINE_PARENT;
  const caller: Caller = { site: "local", run: parent, fn };
  const callId = `${parent}-c1`;
  const child: ChildSpec = { site: "cloud", run: DEADLINE_CHILD, pid: 52_518 };
  const timeoutLine = lineIn(QUOTES_BAML_FILE, fn, "with_timeout");
  const callLine = lineIn(QUOTES_BAML_FILE, fn, callNeedle(VENDOR));

  b.createRun(0, "local", parent, fn, { city });
  startProcess(b, 34, "local", parent, fn, 41_760, true);
  println(b, 40, caller, "asking a slow vendor", `asking a slow vendor for ${city} with a 2 second deadline`);
  b.worker(48, "local", parent, { type: "position", thread: 1, function: fn, file: QUOTES_BAML_FILE, line: timeoutLine, reason: "await", op: null });
  // The work thread runs the body. The deadline thread sleeps for the limit.
  b.worker(50, "local", parent, { type: "thread_started", thread: WORK_THREAD, parent_thread: 1 });
  b.worker(52, "local", parent, { type: "thread_started", thread: DEADLINE_THREAD, parent_thread: 1 });
  b.worker(54, "local", parent, { type: "position", thread: WORK_THREAD, function: fn, file: QUOTES_BAML_FILE, line: callLine, reason: "remote_call", op: null });
  b.worker(55, "local", parent, { type: "remote_call", call_id: callId, thread: WORK_THREAD, function: REMOTE_QUOTE_FN, args: { request: quoteRequest(city, VENDOR) } });
  startQuoteChild(b, 62, caller, callId, child, city, VENDOR);

  // The user pauses the run. The deadline has 1.1 seconds to go.
  const pauseAt = 900;
  b.setStatus(pauseAt, "local", parent, "pausing");
  const builtin = "<builtin>/baml/ns_future/future.baml";
  const main: DumpThread = {
    thread: 1,
    name: "main",
    parked: { kind: "await", detail: "await on the future of the work thread (thread 2)" },
    frames: [
      {
        function: "baml.future.with_timeout",
        file: builtin,
        line: 296,
        locals: [
          { name: "limit", type: "baml.time.Duration", value: dv.instance("baml.time.Duration", { nanoseconds: { kind: "bigint", preview: "2000000000" } }) },
          { name: "token", type: "baml.spawn.CancelToken", value: dv.cancelToken(false) },
          { name: "work", type: "Future<Quote>", value: dv.future(WORK_THREAD, "pending") },
          { name: "deadline", type: "Future<int>", value: dv.future(DEADLINE_THREAD, "pending") },
        ],
      },
      frame(fn, "with_timeout", [["city", "string", dv.str(city)]]),
    ],
  };
  const deadline: DumpThread = {
    thread: DEADLINE_THREAD,
    parent_thread: 1,
    settles_future: DEADLINE_THREAD,
    name: "with_timeout deadline",
    parked: { kind: "sleep", detail: "baml.sys.sleep, 1088 ms remain; the sleep is issued again with the rest at the resume" },
    frames: [{ function: "baml.future.with_timeout.<spawn>", file: builtin, line: 288, locals: [{ name: "token", type: "baml.spawn.CancelToken", value: dv.cancelToken(false) }] }],
  };
  b.snapshot(pauseAt + 12, "local", parent, 1, snapshotStats(12, 61, 10_940), {
    threads: [main, remoteWaitThread(WORK_THREAD, fn, city, VENDOR, callId), deadline],
    heap: heapOf(61, 10_940),
  });

  // The user resumes the run. The deadline keeps counting while no process exists.
  const resumeAt = 1500;
  b.setStatus(resumeAt, "local", parent, "starting", { segment: 2 });
  resumeProcess(b, resumeAt + 38, "local", parent, fn, 41_803);
  for (const thread of [WORK_THREAD, DEADLINE_THREAD]) b.worker(resumeAt + 46, "local", parent, { type: "thread_started", thread, parent_thread: 1 });

  // The deadline passes: the token fires, and the work thread is cancelled in its remote wait.
  const fireAt = 2052;
  cancelQuoteChild(b, fireAt, caller, callId, WORK_THREAD, child);
  b.worker(fireAt + 1, "local", parent, { type: "thread_ended", thread: WORK_THREAD });
  b.worker(fireAt + 2, "local", parent, { type: "thread_ended", thread: DEADLINE_THREAD });
  const value = `no tour quote for ${city}: operation timed out after 2000ms`;
  println(b, fireAt + 5, caller, "baml.io.println(answer)", value);
  b.worker(fireAt + 8, "local", parent, { type: "thread_ended", thread: 1 });
  b.worker(fireAt + 9, "local", parent, { type: "completed", value });
  b.exit(fireAt + 30, "local", parent, 0, "completed", { result: value });

  return b.build("deadline", "with_timeout cancels a remote child when its deadline passes", { roles: { root: { site: "local", id: parent } } });
}
