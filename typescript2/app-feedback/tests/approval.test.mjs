import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { NextRequest } from "next/server";
const jar = new Map();
mock.module("server-only", () => ({}));
mock.module("next/headers", () => ({ cookies: async () => ({
  get: (name) => jar.has(name) ? { value: jar.get(name) } : undefined,
  set: (name, value) => jar.set(name, value),
}) }));
const auth = await import("../src/lib/approval-auth.ts");
const { POST } = await import("../src/app/proposals/[id]/approve/route.ts");
const originalFetch = globalThis.fetch;
const names = ["FEEDBACK_APPROVAL_SESSION_KEY", "FEEDBACK_SITE_URL", "FEEDBACK_SUPABASE_URL", "FEEDBACK_APPROVAL_SUPABASE_KEY"];
const originalEnv = Object.fromEntries(names.map(k => [k, process.env[k]]));
let writes, role;
beforeEach(() => {
  jar.clear(); writes = 0; role = "maintain";
  process.env.FEEDBACK_APPROVAL_SESSION_KEY = "a".repeat(64);
  process.env.FEEDBACK_SITE_URL = "https://feedback.example.invalid";
  process.env.FEEDBACK_SUPABASE_URL = "https://store.example.invalid";
  process.env.FEEDBACK_APPROVAL_SUPABASE_KEY = "offline-fixture";
  globalThis.fetch = mock(async (url, init) => {
    if (url === "https://api.github.com/user") return Response.json({ login: "maintainer" });
    if (url === "https://api.github.com/repos/BoundaryML/baml") return Response.json({ permissions: { [role]: true } });
    if (String(url).startsWith("https://store.example.invalid/")) {
      expect(init.method).toBe("PATCH");
      expect(String(url)).toContain("status=eq.pending&dataset=eq.live");
      expect(String(url)).toContain(`head=eq.${"b".repeat(40)}`);
      expect(JSON.parse(init.body).approved_by).toBe("github:maintainer");
      writes += 1;
      return Response.json(writes === 1 ? [{ id: "p1" }] : []);
    }
    throw new Error("Unexpected network call");
  });
});
afterEach(() => {
  globalThis.fetch = originalFetch;
  for (const k of names) originalEnv[k] === undefined ? delete process.env[k] : process.env[k] = originalEnv[k];
});
function request(origin = "https://feedback.example.invalid", head = "b".repeat(40)) {
  return new NextRequest("https://feedback.example.invalid/proposals/p1/approve", {
    method: "POST", headers: { origin, "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ head }),
  });
}
const params = { params: Promise.resolve({ id: "p1" }) };
test("sessions reject tampering, expiration and the wrong encryption key", () => {
  const sealed = auth.seal({ token: "fixture", expires: Date.now() + 60000 });
  expect(auth.unseal(sealed)?.token).toBe("fixture");
  const bytes = Buffer.from(sealed, "base64url"); bytes[30] ^= 1;
  expect(auth.unseal(bytes.toString("base64url"))).toBeNull();
  expect(auth.unseal(auth.seal({ token: "fixture", expires: 1 }))).toBeNull();
  process.env.FEEDBACK_APPROVAL_SESSION_KEY = "c".repeat(64);
  expect(auth.unseal(sealed)).toBeNull();
});
test("anonymous and cross-origin approvals never write", async () => {
  expect((await POST(request(), params)).status).toBe(403);
  await auth.setSession("fixture");
  expect((await POST(request("https://attacker.example.invalid"), params)).status).toBe(403);
  expect(writes).toBe(0);
});
test("repository role is rechecked and ordinary write access cannot approve", async () => {
  await auth.setSession("fixture"); role = "push";
  expect((await POST(request(), params)).status).toBe(403);
  expect(writes).toBe(0);
});
test("approval records verified actor and uses a one-shot conditional write", async () => {
  await auth.setSession("fixture");
  expect((await POST(request(), params)).status).toBe(303);
  expect((await POST(request(), params)).status).toBe(409);
});
test("invalid head and redirect paths are refused", async () => {
  await auth.setSession("fixture");
  expect((await POST(request(undefined, "bad&status=eq.approved"), params)).status).toBe(400);
  expect(() => auth.proposalPath("//attacker.invalid")).toThrow();
  expect(writes).toBe(0);
});
test("GitHub authorization failures fail closed", async () => {
  await auth.setSession("fixture");
  globalThis.fetch = mock(async () => new Response("unavailable", { status: 503 }));
  expect((await POST(request(), params)).status).toBe(503);
  expect(writes).toBe(0);
});
