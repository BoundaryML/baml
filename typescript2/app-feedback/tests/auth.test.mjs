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
