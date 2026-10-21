// speedtest driver for JavaScript workloads (node and bun):
//   node driver.js MAIN_JS JOBS WARMUP
//
// One call verifies, `warmup - 1` more are untimed, then `jobs` calls are timed
// with a monotonic clock. An `async function main()` is awaited per call. A
// workload that defines `setup()` and `run(state)` has `setup()` called once,
// untimed, and `run(state)` timed instead of `main()`. Prints one JSON report line.
const fs = require("fs");

const [path, jobs, warmup] = [process.argv[2], Number(process.argv[3]), Number(process.argv[4])];
const fns = new Function(`${fs.readFileSync(path, "utf8")}
;return { main, setup: typeof setup === "function" ? setup : null, run: typeof run === "function" ? run : null };`)();
const state = fns.setup && fns.run ? fns.setup() : undefined;
const main = fns.setup && fns.run ? () => fns.run(state) : fns.main;
const isAsync = main.constructor.name === "AsyncFunction";

(async () => {
  const result = String(isAsync ? await main() : main());
  if (isAsync) {
    for (let i = 1; i < warmup; i++) await main();
  } else {
    for (let i = 1; i < warmup; i++) main();
  }
  const start = process.hrtime.bigint();
  if (isAsync) {
    for (let i = 0; i < jobs; i++) await main();
  } else {
    for (let i = 0; i < jobs; i++) main();
  }
  const elapsed = process.hrtime.bigint() - start;
  console.log(JSON.stringify({ result, elapsed_ns: Number(elapsed), jobs }));
})();
