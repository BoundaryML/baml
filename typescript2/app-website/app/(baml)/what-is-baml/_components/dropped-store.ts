// Shared state for the observability joke.
//
// Two counters, both ticking from the moment the page loads:
//   dropped   - only moves in OTEL mode. Freezes when you switch to BAML,
//               because nothing is being dropped any more. The widget at the
//               top of the page uses this.
//   wouldHave - never stops. This is the footnote at the bottom: even once
//               you are on BAML, OTEL would still be losing events.
//
// The interval lives here rather than in a component so the numbers keep
// climbing whether or not either widget is on screen.

const START = 147;
const STEP = 3;
const EVERY_MS = 900;

let baml = false;
let dropped = START;
let wouldHave = START;
let timer: ReturnType<typeof setInterval> | null = null;

const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

function tick() {
  wouldHave += STEP;
  if (!baml) dropped += STEP;
  notify();
}

export function subscribeDropped(fn: () => void) {
  listeners.add(fn);
  if (!timer) timer = setInterval(tick, EVERY_MS);
  return () => {
    listeners.delete(fn);
  };
}

export function getDropped() {
  return dropped;
}

export function getWouldHave() {
  return wouldHave;
}

export function getBaml() {
  return baml;
}

// Server snapshots are the starting values so hydration matches.
export function getStartServer() {
  return START;
}

export function getBamlServer() {
  return false;
}

export function setBaml(next: boolean) {
  baml = next;
  notify();
}
