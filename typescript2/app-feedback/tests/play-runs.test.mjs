import { afterEach, beforeEach, expect, mock, test } from "bun:test";
const jar = new Map();
mock.module("server-only", () => ({}));
mock.module("next/headers", () => ({ cookies: async () => ({
  get: name => jar.has(name) ? { value: jar.get(name) } : undefined,
  set: (name, value) => jar.set(name, value),
}) }));
const auth = await import("../src/lib/approval-auth.ts");
const { playRuns } = await import("../src/lib/play-runs.ts");
const originalFetch = globalThis.fetch;
const names = ["FEEDBACK_APPROVAL_SESSION_KEY", "FEEDBACK_SUPABASE_URL", "FEEDBACK_APPROVAL_SUPABASE_KEY"];
const originalEnv = Object.fromEntries(names.map(k => [k, process.env[k]]));
let reads, role;
beforeEach(() => {
  jar.clear(); reads = []; role = "maintain";
  process.env.FEEDBACK_APPROVAL_SESSION_KEY = "a".repeat(64);
  process.env.FEEDBACK_SUPABASE_URL = "https://store.example.invalid";
  process.env.FEEDBACK_APPROVAL_SUPABASE_KEY = "offline-fixture";
  globalThis.fetch = mock(async (url, init) => {
    if (String(url) === "https://api.github.com/user") return Response.json({ login: "maintainer" });
    if (String(url) === "https://api.github.com/repos/BoundaryML/baml") return Response.json({ permissions: { [role]: true } });
    const target = new URL(url);
    if (target.origin === "https://store.example.invalid") {
      reads.push(target);
      expect(init.cache).toBe("no-store"); expect(init.redirect).toBe("error");
      return Response.json([{ id: 1, feedback_ids: [] }]);
    }
    throw new Error("Unexpected network call");
  });
});
afterEach(() => {
  globalThis.fetch = originalFetch;
  for (const k of names) originalEnv[k] === undefined ? delete process.env[k] : process.env[k] = originalEnv[k];
});
test("anonymous and ordinary contributors cannot read private runs", async () => {
  expect(await playRuns()).toBeNull();
  await auth.setSession("fixture"); role = "push";
  expect(await playRuns("1")).toBeNull(); expect(reads).toHaveLength(0);
});
test("maintainer reads are bounded and transcript is loaded only for a detail page", async () => {
  await auth.setSession("fixture");
  await playRuns(); await playRuns("1");
  expect(reads[0].searchParams.get("limit")).toBe("50");
  expect(reads[0].searchParams.get("select")).not.toContain("transcript");
  expect(reads[1].searchParams.get("select")).toContain("transcript");
  expect(reads[1].searchParams.get("id")).toBe("eq.1");
  expect(reads[1].searchParams.get("kind")).toBe("eq.play");
  expect(reads[1].searchParams.get("dataset")).toBe("eq.live");
});
test("role revocation takes effect before the next private read", async () => {
  await auth.setSession("fixture"); await playRuns(); role = "push";
  expect(await playRuns()).toBeNull(); expect(reads).toHaveLength(1);
});
test("run IDs reject query injection and OAuth redirects reject external paths", async () => {
  for (const id of ["1&select=*", "//example.invalid", "../issues", "0", "1/approve"]) {
    expect(() => auth.runPath(id)).toThrow(); await expect(playRuns(id)).rejects.toThrow();
  }
  expect(auth.runPath()).toBe("/runs"); expect(auth.runPath("12")).toBe("/runs/12");
  expect(reads).toHaveLength(0);
});
