import { test } from "node:test";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { resolveR2Config, R2Store } from "../src/r2.ts";

/**
 * Integration test against the *real* R2 bucket, using the same
 * SCCACHE_* / BAML_SCCACHE_R2_* credentials the action uses at runtime. It
 * writes under a unique throwaway prefix and deletes everything it created, so
 * it never touches real cache objects.
 *
 * Skips (rather than fails) when credentials aren't in the environment — e.g. a
 * fork PR or a checkout without `direnv export`. Run locally after `direnv allow`
 * / `direnv export gha`, or in CI where the secrets are present.
 */
const cfg = resolveR2Config(`__selftest__/${process.pid}-${randomBytes(6).toString("hex")}`);

test("R2Store: put/get/has/del round-trip against real R2", { skip: cfg ? false : "no R2 credentials in env" }, async () => {
  const store = new R2Store(cfg!);

  const key = `roundtrip/${randomBytes(8).toString("hex")}.bin`;
  // Include NUL and high bytes to prove we round-trip raw binary, not text.
  const payload = Buffer.concat([randomBytes(4096), Buffer.from([0x00, 0xff, 0x00, 0x42])]);

  try {
    assert.equal(await store.has(key), false, "object should not exist before put");
    assert.equal(await store.get(key), null, "get of a missing object returns null");

    await store.put(key, payload, "application/x-tar");

    assert.equal(await store.has(key), true, "object should exist after put");
    const got = await store.get(key);
    assert.ok(got, "get should return bytes after put");
    assert.deepEqual(got, payload, "bytes must round-trip exactly");
  } finally {
    await store.del(key);
  }

  // Confirm cleanup actually removed it.
  assert.equal(await store.has(key), false, "object should be gone after del");
});

test("R2Store: list() discovers keys by prefix against real R2", { skip: cfg ? false : "no R2 credentials in env" }, async () => {
  const store = new R2Store(cfg!);

  // Mimic the git-db discovery path: an <ident>-<hash>.tar object found by ident.
  const ident = `aws-sdk-rust-${randomBytes(8).toString("hex")}`;
  const key = `git-db/${ident}.tar`;
  const payload = randomBytes(128);

  try {
    await store.put(key, payload, "application/x-tar");
    const found = await store.list("git-db/aws-sdk-rust-");
    assert.ok(found.includes(key), `list should surface ${key}, got ${JSON.stringify(found)}`);
    // A non-matching prefix returns nothing of ours.
    const none = await store.list("git-db/this-ident-does-not-exist-");
    assert.equal(none.includes(key), false);
  } finally {
    await store.del(key);
  }
});
