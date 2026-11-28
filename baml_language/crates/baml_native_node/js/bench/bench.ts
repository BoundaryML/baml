// One bench for both sides: today's SDK and the native bindings run this same
// file, over the same generated classes and the same BAML functions.
//
//   node dist/bench.js <case> <async|sync> time            median ns per call
//   node dist/bench.js <case> <async|sync> calls <k>       k calls after a warm-up,
//                                                          for counting instructions
//
// The cases named js_* build or read the same objects in plain JavaScript,
// with no bridge: what creating the objects costs by itself.
import "./baml_sdk/index.js";
import { Inner, round_trip_inner, round_trip_inner_sync } from "./baml_sdk/class_refs/index.js";
import {
  round_trip_complex_profile,
  round_trip_complex_profile_sync,
} from "./baml_sdk/complex_models/index.js";
import { round_trip_int, round_trip_int_sync } from "./baml_sdk/primitives/index.js";
import {
  Point,
  count_ints,
  count_ints_sync,
  count_points,
  count_points_sync,
  make_ints,
  make_ints_sync,
  make_points,
  make_points_sync,
} from "./baml_sdk/bridge_test/index.js";
import { complexProfile } from "./bench_values.js";

const INTS = 100_000;
const POINTS = 10_000;

const xs = Array.from({ length: INTS }, (_, i) => i);
const points = Array.from({ length: POINTS }, (_, i) => new Point({ x: i, y: POINTS - i }));
const profile = complexProfile();

let sink: unknown;

type Case = { async: (i: number) => Promise<unknown>; sync: (i: number) => unknown };

const plain = (f: (i: number) => unknown): Case => ({ async: async (i) => f(i), sync: f });

const cases: Record<string, Case> = {
  int: { async: (i) => round_trip_int(i), sync: (i) => round_trip_int_sync(i) },
  inner: {
    async: (i) => round_trip_inner(new Inner({ value: i })),
    sync: (i) => round_trip_inner_sync(new Inner({ value: i })),
  },
  profile: {
    async: () => round_trip_complex_profile(profile),
    sync: () => round_trip_complex_profile_sync(profile),
  },
  ints_out: { async: () => make_ints(INTS), sync: () => make_ints_sync(INTS) },
  ints_in: { async: () => count_ints(xs), sync: () => count_ints_sync(xs) },
  points_out: { async: () => make_points(POINTS), sync: () => make_points_sync(POINTS) },
  points_in: { async: () => count_points(points), sync: () => count_points_sync(points) },
  // No bridge: the same objects built, or read, in JavaScript.
  js_int: plain((i) => i),
  js_inner: plain((i) => new Inner({ value: i })),
  js_profile: plain(() => complexProfile()),
  js_ints_out: plain(() => {
    const a = new Array(INTS);
    for (let i = 0; i < INTS; i++) a[i] = i;
    return a;
  }),
  js_ints_in: plain(() => {
    let n = 0;
    for (let i = 0; i < xs.length; i++) n += xs[i] >= 0 ? 1 : 0;
    return n;
  }),
  js_points_out: plain(() => {
    const a = new Array(POINTS);
    for (let i = 0; i < POINTS; i++) a[i] = new Point({ x: i, y: POINTS - i });
    return a;
  }),
  js_points_in: plain(() => {
    let n = 0;
    for (let i = 0; i < points.length; i++) n += points[i].x + points[i].y > 0 ? 1 : 0;
    return n;
  }),
};

const [name, mode, how, k] = process.argv.slice(2);
const chosen = cases[name];
if (chosen === undefined || (mode !== "async" && mode !== "sync")) {
  console.error(`usage: bench <${Object.keys(cases).join("|")}> <async|sync> <time|calls k>`);
  process.exit(2);
}

async function run(calls: number): Promise<void> {
  if (mode === "async") {
    for (let i = 0; i < calls; i++) sink = await chosen.async(i);
  } else {
    for (let i = 0; i < calls; i++) sink = chosen.sync(i);
  }
}

if (how === "calls") {
  const calls = Number(k);
  await run(Math.max(1, Math.floor(calls / 4))); // warm-up, the same at every k
  await run(calls);
} else {
  // Size a batch to about 100 ms, then the median of nine batches.
  let batch = 1;
  for (;;) {
    const started = process.hrtime.bigint();
    await run(batch);
    const elapsed = Number(process.hrtime.bigint() - started);
    if (elapsed > 50e6 || batch >= 1 << 22) {
      batch = Math.max(1, Math.round((batch * 100e6) / elapsed));
      break;
    }
    batch *= 2;
  }
  const samples: number[] = [];
  for (let round = 0; round < 9; round++) {
    const started = process.hrtime.bigint();
    await run(batch);
    samples.push(Number(process.hrtime.bigint() - started) / batch);
  }
  samples.sort((a, b) => a - b);
  console.log(JSON.stringify({ case: name, mode, batch, ns: samples[4], min: samples[0], max: samples[8] }));
}
if (sink === Symbol.for("never")) console.log(sink);
