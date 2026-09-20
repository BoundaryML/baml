/**
 * Building blocks that the phase 3 fixtures share: the values of the quote
 * program (`quotes.baml.ts`) as JSON and as state dump values, a spawned thread
 * that makes a remote call, and a remote child from its creation to its end.
 */

import type { DumpChild, DumpFrame, DumpThread, DumpValue, Json, JsonObject, PauseStats, ResumeStats, Site, StateDump } from "../protocol";
import { FIXTURE_PROGRAM_HASH, type FixtureBuilder } from "./builder";
import { lineIn } from "./programs";
import { QUOTES_BAML_FILE } from "./quotes.baml";

export const REMOTE_QUOTE_FN = "remote_get_quote";

export type QuoteKind = "Flight" | "Hotel" | "Car" | "Tour";

/** One `remote_get_quote` call of the demo program: `Catalog.request(city, kind, delay_ms)`. */
export interface Vendor {
  kind: QuoteKind;
  delayMs: number;
  /** `"simulate": "unavailable"` in the request options: the vendor throws `QuoteUnavailable`. */
  refuses?: boolean;
}

const VENDOR_NAMES: Record<QuoteKind, string> = { Flight: "Skyways", Hotel: "Casa Azul", Car: "Rodas", Tour: "Seven Hills Walks" };
const NIGHTLY_RATES: Record<QuoteKind, number> = { Flight: 420, Hotel: 135, Car: 48, Tour: 75 };
const NIGHTS = 3;
const NOTE = "loyalty tier 2 applied for Ada";

export const vendorName = (vendor: Vendor): string => VENDOR_NAMES[vendor.kind];
/** A flight is priced once. Everything else is priced per night. */
export const quoteAmount = (vendor: Vendor): number => (vendor.kind === "Flight" ? NIGHTLY_RATES.Flight : NIGHTLY_RATES[vendor.kind] * NIGHTS);
/** The text in the program that names the call, for the line lookup. */
export const callNeedle = (vendor: Vendor): string => (vendor.refuses ? "remote_get_quote(car_request)" : `QuoteKind.${vendor.kind}, ${vendor.delayMs}`);
/** The error text of a vendor that refuses, as the child's `failed` event carries it. */
export const refusalOf = (city: string, vendor: Vendor): string => `no ${vendor.kind} vendor answers in ${city}`;

// ---------------------------------------------------------------------------
// Values as JSON, the way a `completed` event and a `remote_call` event carry them
// ---------------------------------------------------------------------------

export function quoteRequest(city: string, vendor: Vendor): JsonObject {
  return {
    city,
    kind: vendor.kind,
    nights: NIGHTS,
    delay_ms: vendor.delayMs,
    traveler: { name: "Ada", loyalty_tier: 2 },
    options: vendor.refuses ? { currency: "EUR", simulate: "unavailable" } : { currency: "EUR" },
  };
}

export function quoteValue(city: string, vendor: Vendor): JsonObject {
  return {
    request: quoteRequest(city, vendor),
    vendor: vendorName(vendor),
    price: { amount: quoteAmount(vendor), currency: "EUR" },
    tags: [vendor.kind.toLowerCase(), city.toLowerCase()],
    extras: { insurance: 12, late_checkout: 30 },
    note: NOTE,
  };
}

// ---------------------------------------------------------------------------
// Values as state dump values (section 2.5)
// ---------------------------------------------------------------------------

const children = (entries: Record<string, DumpValue>): DumpChild[] => Object.entries(entries).map(([key, value]) => ({ key, value }));

export const dv = {
  str: (value: string): DumpValue => ({ kind: "string", preview: JSON.stringify(value) }),
  int: (value: number): DumpValue => ({ kind: "int", preview: String(value) }),
  bool: (value: boolean): DumpValue => ({ kind: "bool", preview: String(value) }),
  nil: (): DumpValue => ({ kind: "null", preview: "null" }),
  /** The snapshot core renders an enum variant as an opaque value named `Enum.Variant`. */
  variant: (enumName: string, variant: string): DumpValue => ({ kind: "opaque", preview: `${enumName}.${variant}` }),
  instance: (className: string, fields: Record<string, DumpValue>): DumpValue => ({
    kind: "instance",
    class: className,
    preview: className,
    children: children(fields),
  }),
  map: (keyType: string, valueType: string, entries: Record<string, DumpValue>): DumpValue => ({
    kind: "map",
    preview: `map<${keyType}, ${valueType}> (len ${Object.keys(entries).length})`,
    children: children(entries),
  }),
  array: (elementType: string, items: DumpValue[]): DumpValue => ({
    kind: "array",
    preview: `${elementType}[] (len ${items.length})`,
    children: items.map((value, index) => ({ key: `[${index}]`, value })),
  }),
  /**
   * A future in the form of the snapshot core: the preview is `#<id> (<state>)`,
   * and a settled future has one child, `value` or `error`.
   */
  future: (id: number, state: "pending" | "ready" | "failed" | "cancelled", settled?: DumpValue): DumpValue => ({
    kind: "future",
    preview: `#${id} (${state})`,
    ...(settled === undefined ? {} : { children: [{ key: state === "failed" ? "error" : "value", value: settled }] }),
  }),
  /** A cancel token in the form of the snapshot core. */
  cancelToken: (cancelled: boolean): DumpValue => ({ kind: "cancel_token", preview: `<cancel token: ${cancelled ? "cancelled" : "not cancelled"}>` }),
};

export function moneyDump(amount: number): DumpValue {
  return dv.instance("Money", { amount: dv.int(amount), currency: dv.str("EUR") });
}

/** A `QuoteRequest`: an enum, a nested class with an optional field, and a map. */
export function requestDump(city: string, vendor: Vendor): DumpValue {
  return dv.instance("QuoteRequest", {
    city: dv.str(city),
    kind: dv.variant("QuoteKind", vendor.kind),
    nights: dv.int(NIGHTS),
    delay_ms: dv.int(vendor.delayMs),
    traveler: dv.instance("Traveler", { name: dv.str("Ada"), loyalty_tier: dv.int(2) }),
    options: dv.map("string", "string", vendor.refuses ? { currency: dv.str("EUR"), simulate: dv.str("unavailable") } : { currency: dv.str("EUR") }),
  });
}

/** A `Quote`: the request it answers, a nested class, an array, a map, and an optional field. */
export function quoteDump(city: string, vendor: Vendor): DumpValue {
  return dv.instance("Quote", {
    request: requestDump(city, vendor),
    vendor: dv.str(vendorName(vendor)),
    price: moneyDump(quoteAmount(vendor)),
    tags: dv.array("string", [dv.str(vendor.kind.toLowerCase()), dv.str(city.toLowerCase())]),
    extras: dv.map("string", "int", { insurance: dv.int(12), late_checkout: dv.int(30) }),
    note: dv.str(NOTE),
  });
}

export function frame(fn: string, needle: string, locals: [string, string | null, DumpValue][], searchIn = fn): DumpFrame {
  return {
    function: fn,
    file: QUOTES_BAML_FILE,
    line: lineIn(QUOTES_BAML_FILE, searchIn, needle),
    locals: locals.map(([name, type, value]) => ({ name, type, value })),
  };
}

/** A spawned thread that waits for the result of its remote call. */
export function remoteWaitThread(thread: number, fn: string, city: string, vendor: Vendor, callId: string): DumpThread {
  return {
    thread,
    parent_thread: 1,
    // The fixtures number a future after the thread that settles it.
    settles_future: thread,
    cancelled: false,
    name: `spawn ${vendor.kind.toLowerCase()}`,
    parked: { kind: "remote_call", detail: `${REMOTE_QUOTE_FN} as ${callId}; the call can be issued again after a resume` },
    frames: [
      frame(`${fn}.<spawn>`, callNeedle(vendor), [["city", "string", dv.str(city)], ["request", "QuoteRequest", requestDump(city, vendor)]], fn),
    ],
  };
}

export function heapOf(objects: number, bytes: number): StateDump["heap"] {
  return {
    objects,
    bytes,
    by_kind: {
      instance: { count: Math.round(objects * 0.3), bytes: Math.round(bytes * 0.34) },
      string: { count: Math.round(objects * 0.36), bytes: Math.round(bytes * 0.27) },
      future: { count: Math.round(objects * 0.1), bytes: Math.round(bytes * 0.14) },
      closure: { count: Math.round(objects * 0.12), bytes: Math.round(bytes * 0.13) },
      map: { count: Math.round(objects * 0.06), bytes: Math.round(bytes * 0.07) },
      array: { count: Math.round(objects * 0.06), bytes: Math.round(bytes * 0.05) },
    },
  };
}

export function snapshotStats(latencyMs: number | null, objects: number, raw: number): PauseStats {
  return {
    pause_latency_ms: latencyMs,
    walk_ms: 0.6 + objects * 0.004,
    encode_ms: 1.1 + objects * 0.006,
    compress_ms: 2.0 + objects * 0.008,
    write_ms: 2.8,
    objects,
    raw_bytes: raw,
    compressed_bytes: Math.round(raw * 0.44),
    program_bytes: 24_610,
    blocked_attempts: 0,
  };
}

/** Resume timings of a worker that loaded the program from the program store (section 9.5). */
export const RESUME_FROM_STORE: ResumeStats = { process_start_ms: 21.4, program_load_ms: 3.2, decode_ms: 2.4, first_exec_ms: 28.9, program_source: "store" };

// ---------------------------------------------------------------------------
// Event sequences
// ---------------------------------------------------------------------------

export interface Caller {
  site: Site;
  run: string;
  fn: string;
}

/** `hello` and the root thread of a new process. */
export function startProcess(b: FixtureBuilder, at: number, site: Site, run: string, fn: string, pid: number, durable: boolean): void {
  b.setStatus(at, site, run, "running", { pid });
  b.worker(at + 2, site, run, { type: "hello", mode: "start", function: fn, durable, program_hash: FIXTURE_PROGRAM_HASH });
  b.worker(at + 3, site, run, { type: "thread_started", thread: 1, parent_thread: null });
}

/** `hello`, `resumed`, and the root thread of a process that restores a snapshot. */
export function resumeProcess(b: FixtureBuilder, at: number, site: Site, run: string, fn: string, pid: number, stats: ResumeStats = RESUME_FROM_STORE): void {
  b.setStatus(at, site, run, "running", { pid });
  b.worker(at + 2, site, run, { type: "hello", mode: "resume", function: fn, durable: true, program_hash: FIXTURE_PROGRAM_HASH });
  b.worker(at + 6, site, run, { type: "resumed", stats });
  b.worker(at + 6, site, run, { type: "thread_started", thread: 1, parent_thread: null });
}

export function println(b: FixtureBuilder, at: number, caller: Caller, needle: string, text: string, thread = 1): void {
  const line = lineIn(QUOTES_BAML_FILE, caller.fn, needle);
  b.worker(at, caller.site, caller.run, { type: "position", thread, function: caller.fn, file: QUOTES_BAML_FILE, line, reason: "sysop", op: "baml.io.println" });
  b.worker(at + 1, caller.site, caller.run, { type: "log", stream: "stdout", text, thread });
}

/** `spawn { remote_get_quote(...) }`: the new thread, its position, and its `remote_call`. */
export function spawnQuoteCall(b: FixtureBuilder, at: number, caller: Caller, thread: number, callId: string, city: string, vendor: Vendor, onLineOf: Vendor = vendor): void {
  const line = lineIn(QUOTES_BAML_FILE, caller.fn, callNeedle(onLineOf));
  b.worker(at, caller.site, caller.run, { type: "thread_started", thread, parent_thread: 1 });
  b.worker(at + 2, caller.site, caller.run, { type: "position", thread, function: caller.fn, file: QUOTES_BAML_FILE, line, reason: "remote_call", op: null });
  b.worker(at + 3, caller.site, caller.run, { type: "remote_call", call_id: callId, thread, function: REMOTE_QUOTE_FN, args: { request: quoteRequest(city, vendor) } });
}

export interface ChildSpec {
  site: Site;
  run: string;
  pid: number;
}

/**
 * The start of a remote child: the record on its site, the dispatch on the
 * parent's site, the process, and the vendor's sleep. The child's site has no
 * copy of the project. It runs the program from its program store under the
 * hash that the parent's site sent (section 9.5). Returns the time at which the
 * vendor's sleep ends.
 */
export function startQuoteChild(b: FixtureBuilder, at: number, caller: Caller, callId: string, child: ChildSpec, city: string, vendor: Vendor): number {
  b.createRun(at, child.site, child.run, REMOTE_QUOTE_FN, { request: quoteRequest(city, vendor) }, { parent: { site: caller.site, run: caller.run, call_id: callId } });
  b.siteEvent(at + 5, { type: "remote_dispatched", site: caller.site, run: caller.run, call_id: callId, child_site: child.site, child_run: child.run, function: REMOTE_QUOTE_FN });
  const parent = b.run(caller.site, caller.run);
  b.update(at + 5, caller.site, caller.run, {
    waiting_on: [...parent.waiting_on, { call_id: callId, child_site: child.site, child_run: child.run, function: REMOTE_QUOTE_FN }],
  });
  startProcess(b, at + 40, child.site, child.run, REMOTE_QUOTE_FN, child.pid, false);
  const self: Caller = { site: child.site, run: child.run, fn: REMOTE_QUOTE_FN };
  println(b, at + 46, self, "baml.io.println", `[remote] ${vendor.kind} quote for ${city} (${vendor.delayMs} ms)`);
  b.worker(at + 48, child.site, child.run, {
    type: "position", thread: 1, function: REMOTE_QUOTE_FN, file: QUOTES_BAML_FILE,
    line: lineIn(QUOTES_BAML_FILE, REMOTE_QUOTE_FN, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep",
  });
  return at + 48 + vendor.delayMs;
}

/** The parent's site stores or delivers a result: `remote_returned`, and the entry leaves `waiting_on`. */
function returned(b: FixtureBuilder, at: number, caller: Caller, callId: string, child: ChildSpec, ok: boolean, value: Json, error: string | null, delivered: boolean): void {
  const parent = b.run(caller.site, caller.run);
  b.siteEvent(at, { type: "remote_returned", site: caller.site, run: caller.run, call_id: callId, child_site: child.site, child_run: child.run, ok });
  b.update(at, caller.site, caller.run, {
    waiting_on: parent.waiting_on.filter((entry) => entry.call_id !== callId),
    remote_results: [...parent.remote_results, { call_id: callId, value, error, acked: delivered, ts: b.ts(at) }],
  });
  b.update(at + 2, child.site, child.run, { result_delivered: true });
}

/** The child completes and its result reaches the parent's site. Returns the time of `remote_returned`. */
export function completeQuoteChild(b: FixtureBuilder, at: number, caller: Caller, callId: string, child: ChildSpec, city: string, vendor: Vendor, delivered: boolean): number {
  const value = quoteValue(city, vendor);
  b.worker(at, child.site, child.run, { type: "thread_ended", thread: 1 });
  b.worker(at + 1, child.site, child.run, { type: "completed", value });
  b.exit(at + 4, child.site, child.run, 0, "completed", { result: value });
  returned(b, at + 12, caller, callId, child, true, value, null, delivered);
  return at + 12;
}

/** The child throws a typed error, which reaches the parent's site as a failed result. */
export function failQuoteChild(b: FixtureBuilder, at: number, caller: Caller, callId: string, child: ChildSpec, error: string, delivered: boolean): number {
  b.worker(at, child.site, child.run, {
    type: "failed",
    error,
    stack: [{ function: REMOTE_QUOTE_FN, file: QUOTES_BAML_FILE, line: lineIn(QUOTES_BAML_FILE, REMOTE_QUOTE_FN, "throw QuoteUnavailable") }],
  });
  b.exit(at + 3, child.site, child.run, 1, "failed", { error });
  returned(b, at + 11, caller, callId, child, false, null, error, delivered);
  return at + 11;
}

/**
 * A remote cancellation (sections 9.2 and 9.3): the worker reports
 * `remote_cancel`, the parent's site removes the entry from `waiting_on`, asks
 * the child's site to cancel the child, and emits `remote_cancelled`. The
 * child's worker emits `cancelled` and exits with code 130.
 */
export function cancelQuoteChild(b: FixtureBuilder, at: number, caller: Caller, callId: string, thread: number, child: ChildSpec): number {
  b.worker(at, caller.site, caller.run, { type: "remote_cancel", call_id: callId, thread });
  const parent = b.run(caller.site, caller.run);
  b.update(at + 2, caller.site, caller.run, { waiting_on: parent.waiting_on.filter((entry) => entry.call_id !== callId) });
  b.siteEvent(at + 3, { type: "remote_cancelled", site: caller.site, run: caller.run, call_id: callId, child_site: child.site, child_run: child.run });
  b.worker(at + 21, child.site, child.run, { type: "cancelled" });
  b.exit(at + 24, child.site, child.run, 130, "cancelled");
  return at + 24;
}
