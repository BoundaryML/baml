import { expect, test } from "bun:test";
import { signedSession, verifySession } from "../src/lib/auth.ts";

test("sessions require a valid signature and expire; no raw identity cookie is trusted", () => {
  const old = process.env.ATB2_UI_SESSION_SECRET;
  process.env.ATB2_UI_SESSION_SECRET = "offline-session-fixture-key-00000000000000";
  try {
    const token = signedSession("fixture-user");
    expect(verifySession(token)).toBe("fixture-user");
    expect(verifySession(token + ".extra")).toBeNull();
    expect(verifySession("fixture-user")).toBeNull();
    expect(verifySession(token.replace(/.$/, "!"))).toBeNull();
    const now = Date.now;
    Date.now = () => now() + 3_600_001;
    try { expect(verifySession(token)).toBeNull(); } finally { Date.now = now; }
  } finally { if (old === undefined) delete process.env.ATB2_UI_SESSION_SECRET; else process.env.ATB2_UI_SESSION_SECRET = old; }
});

test("OAuth starts on the callback host so aliases cannot lose the state cookie", async () => {
  const { NextRequest } = await import("next/server");
  const { GET } = await import("../src/app/api/auth/github/route.ts");
  const keys = ["ATB2_UI_URL", "ATB2_GITHUB_CLIENT_ID", "ATB2_GITHUB_CLIENT_SECRET", "ATB2_UI_SESSION_SECRET"];
  const previous = Object.fromEntries(keys.map(k => [k, process.env[k]]));
  Object.assign(process.env, { ATB2_UI_URL: "https://feedback.example", ATB2_GITHUB_CLIENT_ID: "fixture", ATB2_GITHUB_CLIENT_SECRET: "fixture-secret", ATB2_UI_SESSION_SECRET: "offline-session-fixture-key-00000000000000" });
  try {
    const alias = await GET(new NextRequest("https://alias.example/api/auth/github"));
    expect(alias.headers.get("location")).toBe("https://feedback.example/api/auth/github");
    expect(alias.headers.has("set-cookie")).toBe(false);
    const canonical = await GET(new NextRequest("https://feedback.example/api/auth/github"));
    const location = new URL(canonical.headers.get("location"));
    expect(location.origin).toBe("https://github.com");
    expect(location.searchParams.get("redirect_uri")).toBe("https://feedback.example/api/auth/github/callback");
    expect(canonical.headers.get("set-cookie")).toContain("HttpOnly");
    expect(canonical.headers.get("set-cookie")).toContain("Secure");
    expect(canonical.headers.get("cache-control")).toBe("no-store");
    expect(location.search).not.toContain("fixture-secret");
  } finally {
    for (const k of keys) { if (previous[k] === undefined) delete process.env[k]; else process.env[k] = previous[k]; }
  }
});
