// Tables from the Node bench's results (scripts/bench.sh).
//
//   node scripts/bench_summary.mjs [target/native-node]
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "target/native-node";
const read = (file) =>
  existsSync(join(dir, file))
    ? readFileSync(join(dir, file), "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l))
    : [];

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

const time = new Map();
for (const r of read("bench-time.jsonl")) {
  const key = `${r.config}|${r.case}|${r.mode}`;
  if (!time.has(key)) time.set(key, []);
  time.get(key).push(r.ns);
}
const instructions = new Map();
for (const r of read("bench-instructions.jsonl")) {
  instructions.set(`${r.config}|${r.case}|${r.mode}`, (r.high - r.low) / (2 * r.k));
}

const SIZES = { ints_out: 100_000, ints_in: 100_000, points_out: 10_000, points_in: 10_000 };
const LABELS = {
  int: "`round_trip_int`",
  inner: "`round_trip_inner`",
  profile: "`round_trip_complex_profile`",
  ints_out: "`int[]` out, 100,000",
  ints_in: "`int[]` in, 100,000",
  points_out: "`Point[]` out, 10,000",
  points_in: "`Point[]` in, 10,000",
};

function fmtTime(ns) {
  if (ns === undefined) return "";
  if (ns >= 1e6) return `${(ns / 1e6).toFixed(2)} ms`;
  if (ns >= 1e3) return `${(ns / 1e3).toFixed(ns >= 1e5 ? 0 : 1)} µs`;
  return `${ns.toFixed(0)} ns`;
}

function fmtIns(n) {
  if (n === undefined) return "";
  return Math.round(n).toLocaleString("en-US");
}

const t = (config, c, mode) => {
  const xs = time.get(`${config}|${c}|${mode}`);
  return xs ? median(xs) : undefined;
};
const i = (config, c, mode) => instructions.get(`${config}|${c}|${mode}`);

function cell(config, c, mode) {
  const ns = t(config, c, mode);
  const ins = i(config, c, mode);
  if (ns === undefined) return "";
  return ins === undefined ? fmtTime(ns) : `${fmtTime(ns)} · ${fmtIns(ins)}`;
}

function perElement(config, c, mode) {
  const ns = t(config, c, mode);
  return ns === undefined || !SIZES[c] ? "" : `${(ns / SIZES[c]).toFixed(1)} ns`;
}

const cases = Object.keys(LABELS);
const out = [];
out.push("Each cell: time per call (median of rounds, each the median of nine batches) · instructions per call (two call counts, the difference).\n");

out.push("**Async against async** (`await f(...)`, one call after another)\n");
out.push("| Case | Today's SDK | Native, eager turn | Native, microtask turn | Plain JavaScript, no bridge | Native vs today |");
out.push("|---|---|---|---|---|---|");
for (const c of cases) {
  const today = t("today", c, "async");
  const native = t("native-eager", c, "async");
  const ratio = today && native ? `${(today / native).toFixed(1)}x faster` : "";
  out.push(
    `| ${LABELS[c]} | ${cell("today", c, "async")} | ${cell("native-eager", c, "async")} | ${cell("native-microtask", c, "async")} | ${cell("plain-js", `js_${c}`, "sync")} | ${ratio} |`,
  );
}

out.push("\n**Sync against sync** (`f_sync(...)`)\n");
out.push("| Case | Today's SDK | Native | Plain JavaScript, no bridge | Native vs today |");
out.push("|---|---|---|---|---|");
for (const c of cases) {
  const today = t("today", c, "sync");
  const native = t("native-eager", c, "sync");
  const ratio = today && native ? `${(today / native).toFixed(1)}x faster` : "";
  out.push(
    `| ${LABELS[c]} | ${cell("today", c, "sync")} | ${cell("native-eager", c, "sync")} | ${cell("plain-js", `js_${c}`, "sync")} | ${ratio} |`,
  );
}

out.push("\n**Per element** (async)\n");
out.push("| Case | Today's SDK | Native | Plain JavaScript |");
out.push("|---|---|---|---|");
for (const c of Object.keys(SIZES)) {
  out.push(
    `| ${LABELS[c]} | ${perElement("today", c, "async")} | ${perElement("native-eager", c, "async")} | ${perElement("plain-js", `js_${c}`, "sync") || (t("plain-js", `js_${c}`, "sync") ? `${(t("plain-js", `js_${c}`, "sync") / SIZES[c]).toFixed(1)} ns` : "")} |`,
  );
}

out.push("\n**Building class instances** (async, eager turn)\n");
out.push("| Case | Set each property from native code | Per-class factory, values in order | Per-class factory, `new C({..})` as today | Today's SDK | Plain JavaScript |");
out.push("|---|---|---|---|---|---|");
for (const c of ["inner", "profile", "points_out"]) {
  out.push(
    `| ${LABELS[c]} | ${cell("native-props", c, "async")} | ${cell("native-eager", c, "async")} | ${cell("native-init", c, "async")} | ${cell("today", c, "async")} | ${cell("plain-js", `js_${c}`, "sync")} |`,
  );
}
console.log(out.join("\n"));
