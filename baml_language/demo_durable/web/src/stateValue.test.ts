import { describe, expect, it } from "vitest";
import { buildDeadlineFixture, DEADLINE_PARENT } from "./fixtures/deadline";
import { buildFanoutFixture, FANOUT_PARENT } from "./fixtures/fanout";
import { dv, quoteDump } from "./fixtures/quotes";
import { buildSettledFixture, SETTLED_PARENT } from "./fixtures/settled";
import { buildSpawnFixture, SPAWN_PARENT } from "./fixtures/spawn";
import type { DumpValue, StateDump } from "./protocol";
import { futureStateOf, inlineSummary, orderedThreads, shapeOf } from "./stateValue";

const flight = { kind: "Flight", delayMs: 2000 } as const;

function localsOf(dump: StateDump | undefined, thread: number, frame = 0): Record<string, DumpValue> {
  const locals = dump?.threads.find((candidate) => candidate.thread === thread)?.frames[frame]?.locals ?? [];
  return Object.fromEntries(locals.map((local) => [local.name, local.value]));
}

describe("futures in the state dump", () => {
  const settled = buildSettledFixture().states[`local/${SETTLED_PARENT}/1`];

  it("reads the three states of the settled fixture with the value or the error", () => {
    const locals = localsOf(settled, 1);
    const resolved = shapeOf(locals["flight"] as DumpValue);
    // The form of the snapshot core: `#<id> (<state>)`, without a type.
    expect(resolved).toMatchObject({ shape: "future", state: "resolved", id: 2, type: null });
    expect(resolved.shape === "future" ? resolved.settled?.key : null).toBe("value");
    expect(resolved.shape === "future" ? resolved.settled?.value.class : null).toBe("Quote");
    const failed = shapeOf(locals["car"] as DumpValue);
    expect(failed).toMatchObject({ shape: "future", state: "failed" });
    // The typed error of the child crossed the sites as text, so the parent holds a `baml.errors.Io`.
    expect(failed.shape === "future" ? [failed.settled?.key, failed.settled?.value.class] : null).toEqual(["error", "baml.errors.Io"]);
    expect(shapeOf(locals["tour"] as DumpValue)).toMatchObject({ shape: "future", state: "pending", id: 4, settled: null });
    expect(inlineSummary(locals["tour"] as DumpValue)).toBe("Future #4 pending");
    // A long text inside a value is cut with an ellipsis. The tree shows it in full when the value is opened.
    expect(inlineSummary(locals["car"] as DumpValue, 200)).toBe('Future #3 failed = Io { message: "QuoteUnavailable { kind: Car, city: \\"Lisbon\\"… }');
    // In a narrow place an error keeps its message.
    expect(inlineSummary(locals["car"] as DumpValue, 60)).toBe("Future #3 failed = Io: QuoteUnavailable { kind: Car, city: …");
    // The thread that settles a future names its id.
    expect(settled?.threads.map((thread) => [thread.thread, thread.settles_future ?? null])).toEqual([[1, null], [4, 4], [5, 5]]);
  });

  it("reads the other states of the snapshot core", () => {
    expect(shapeOf({ kind: "future", preview: "#9 (cancelled)", children: [] })).toMatchObject({ state: "cancelled", id: 9 });
    expect(shapeOf({ kind: "future", preview: "#10 (internal error)", children: [] })).toMatchObject({ state: "failed", id: 10 });
    expect(shapeOf({ kind: "future", preview: "#11 (ready)", children: [{ key: "value", value: dv.nil() }] })).toMatchObject({ state: "resolved", settled: { key: "value" } });
  });

  it("finds futures inside an array and inside the outcomes of all_settled", () => {
    const outcomes = localsOf(settled, 5)["outcomes"] as DumpValue;
    expect(shapeOf(outcomes)).toEqual({ shape: "array", size: 2 });
    expect(outcomes.children?.map((child) => shapeOf(child.value))).toEqual([
      { shape: "instance", className: "baml.future.Success<Quote>" },
      { shape: "instance", className: "baml.future.Failure<QuoteUnavailable | Io>" },
    ]);
    expect(shapeOf(localsOf(settled, 5)["f"] as DumpValue)).toMatchObject({ shape: "future", state: "pending" });
  });

  it("holds four pending futures and five threads in the snapshot of the fan-out", () => {
    const dump = buildFanoutFixture().states[`local/${FANOUT_PARENT}/1`];
    expect(dump?.threads.map((thread) => thread.parked.kind)).toEqual(["sleep", "remote_call", "remote_call", "remote_call", "remote_call"]);
    const futures = Object.values(localsOf(dump, 1)).map(shapeOf).filter((shape) => shape.shape === "future");
    expect(futures.map((shape) => (shape.shape === "future" ? shape.state : null))).toEqual(["pending", "pending", "pending", "pending"]);
  });

  it("tolerates the other forms a worker can use for a future", () => {
    // An instance of `Future` with a quoted state, as the fixture of phase 1 has it.
    const spawn = localsOf(buildSpawnFixture().states[`local/${SPAWN_PARENT}/1`], 1)["forecast"] as DumpValue;
    expect(shapeOf(spawn)).toMatchObject({ shape: "future", state: "resolved" });
    // An opaque value with nothing but a preview.
    expect(shapeOf({ kind: "opaque", preview: "<future pending>" })).toMatchObject({ shape: "future", state: "pending", settled: null });
    expect(shapeOf({ kind: "opaque", preview: "Future<int> cancelled" })).toMatchObject({ shape: "future", state: "cancelled", type: "Future<int>" });
    // A namespaced class with type arguments, and a state enum of another spelling.
    const namespaced: DumpValue = { kind: "instance", class: "baml.future.Future<Quote, VendorDown>", preview: "baml.future.Future", children: [{ key: "status", value: dv.variant("State", "Error") }, { key: "error", value: dv.str("boom") }] };
    expect(shapeOf(namespaced)).toMatchObject({ shape: "future", state: "failed", type: "Future<Quote, VendorDown>" });
    // A kind that the app has never seen stays a plain value.
    expect(shapeOf({ kind: "hologram", preview: "?" })).toEqual({ shape: "plain" });
  });

  it("maps the state words of the runtime, including a settled error that nobody observed yet", () => {
    expect(futureStateOf("baml.future.State.Pending")).toBe("pending");
    expect(futureStateOf("Ready")).toBe("resolved");
    expect(futureStateOf('"resolved"')).toBe("resolved");
    expect(futureStateOf("ErrorPending")).toBe("failed");
    expect(futureStateOf("Panicked")).toBe("failed");
    expect(futureStateOf("Cancelled")).toBe("cancelled");
    expect(futureStateOf("Tier.Budget")).toBeNull();
  });
});

describe("cancel tokens, enums, maps, and nested instances", () => {
  it("reads a cancel token and its state", () => {
    const token = localsOf(buildDeadlineFixture().states[`local/${DEADLINE_PARENT}/1`], 1)["token"] as DumpValue;
    expect(shapeOf(token)).toEqual({ shape: "cancel_token", cancelled: false, sources: 0 });
    expect(shapeOf(dv.cancelToken(true))).toEqual({ shape: "cancel_token", cancelled: true, sources: 0 });
    // `CancelToken.any` names the number of its sources.
    expect(shapeOf({ kind: "cancel_token", preview: "<cancel token: not cancelled, any of 2>" })).toEqual({ shape: "cancel_token", cancelled: false, sources: 2 });
    // Another worker can render the token as an instance, with or without a flag.
    expect(shapeOf({ kind: "instance", class: "CancelToken", preview: "CancelToken" })).toEqual({ shape: "cancel_token", cancelled: null, sources: 0 });
    expect(shapeOf(dv.instance("baml.spawn.CancelToken", { cancelled: dv.bool(true) }))).toMatchObject({ cancelled: true });
    expect(inlineSummary(dv.cancelToken(false))).toBe("CancelToken armed");
  });

  it("reads a task group with its limit and its queue", () => {
    expect(shapeOf({ kind: "task_group", preview: '<task group "vendors", limit 2, 1 running, 3 queued>' })).toEqual({ shape: "task_group", summary: '"vendors", limit 2, 1 running, 3 queued' });
    const group = dv.instance("baml.spawn.TaskGroup", { name: dv.str("vendors"), limit: dv.int(2), active_count: dv.int(2), queued_count: dv.int(5) });
    expect(shapeOf(group)).toEqual({ shape: "task_group", summary: "name vendors · limit 2 · active 2 · queued 5" });
  });

  it("reads an enum variant from the opaque form of the snapshot core", () => {
    expect(shapeOf(dv.variant("QuoteKind", "Hotel"))).toEqual({ shape: "enum", enumName: "QuoteKind", variant: "Hotel" });
    expect(shapeOf(dv.variant("baml.future.State", "Pending"))).toEqual({ shape: "enum", enumName: "baml.future.State", variant: "Pending" });
    expect(inlineSummary(dv.variant("baml.future.State", "Pending"))).toBe("State.Pending");
    // Other opaque values are not enums.
    expect(shapeOf({ kind: "opaque", preview: "<host resource>" })).toEqual({ shape: "plain" });
    expect(shapeOf({ kind: "opaque", preview: "1.5" })).toEqual({ shape: "plain" });
  });

  it("summarizes a nested instance with its enum, its nested classes, its maps, its array, and its optional field on one line", () => {
    const quote = quoteDump("Lisbon", flight);
    expect(shapeOf(quote)).toEqual({ shape: "instance", className: "Quote" });
    // A nested value that does not fit is named, not cut in the middle.
    expect(inlineSummary(quote, 400)).toBe(
      'Quote { request: QuoteRequest {…}, vendor: "Skyways", price: Money { amount: 420, currency: "EUR" }, tags: [2 items], extras: {2 entries}, note: "loyalty tier 2 applied for Ada" }',
    );
    expect(inlineSummary(quote, 40)).toHaveLength(40);
    const field = (name: string): DumpValue => quote.children?.find((child) => child.key === name)?.value as DumpValue;
    // The request: an enum, a nested class with an optional field, and a map.
    expect(inlineSummary(field("request"), 400)).toBe(
      'QuoteRequest { city: "Lisbon", kind: QuoteKind.Flight, nights: 3, delay_ms: 2000, traveler: Traveler { name: "Ada", loyalty_tier: 2 }, options: {1 entry} }',
    );
    expect(shapeOf(field("extras"))).toEqual({ shape: "map", size: 2 });
    expect(inlineSummary(field("extras"))).toBe('{ "insurance": 12, "late_checkout": 30 }');
    expect(inlineSummary(field("tags"))).toBe('["flight", "lisbon"]');
    expect(inlineSummary(dv.instance("Traveler", { name: dv.str("Ada"), loyalty_tier: dv.nil() }))).toBe('Traveler { name: "Ada", loyalty_tier: null }');
  });

  it("reads the size of an array from either preview form and stops at a cycle", () => {
    expect(shapeOf({ kind: "array", preview: "[3 items]" })).toEqual({ shape: "array", size: 3 });
    expect(shapeOf(dv.array("Quote", [quoteDump("Lisbon", flight)]))).toEqual({ shape: "array", size: 1 });
    const node = dv.instance("Node", { name: dv.str("a"), next: dv.instance("Node", { name: dv.str("b"), next: { kind: "omitted", preview: "<cycle>" } }) });
    expect(inlineSummary(node, 200)).toBe('Node { name: "a", next: Node { name: "b", next: <cycle> } }');
  });
});

describe("the order of the threads in the state tree", () => {
  it("puts the root thread first and the spawned threads after it by id, whatever order the worker wrote", () => {
    // The real worker writes the threads in the order in which they parked: 3, 5, 4, 6, and the root 2 last.
    const thread = (id: number, parent: number | null) => ({ thread: id, name: `t${id}`, parked: { kind: "remote_call", detail: "" }, frames: [], parent_thread: parent });
    const dump = { run: "r", segment: 1, created_ts: 0, threads: [thread(3, 2), thread(5, 2), thread(4, 2), thread(6, 2), thread(2, null)], heap: { objects: 0, bytes: 0, by_kind: {} } } as StateDump;
    expect(orderedThreads(dump).map((entry) => entry.thread)).toEqual([2, 3, 4, 5, 6]);
    // A dump without parent_thread fields (a worker that predates section 9) keeps the order of the ids.
    const legacy = { ...dump, threads: dump.threads.map(({ parent_thread: _parent, ...rest }) => rest) } as StateDump;
    expect(orderedThreads(legacy).map((entry) => entry.thread)).toEqual([2, 3, 4, 5, 6]);
  });
});
