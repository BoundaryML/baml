import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { R2Store } from "../src/r2.ts";

/**
 * Regression tests for the "Error: write EOF" crash seen in CI: a recycled
 * keep-alive socket dies and the resulting socket 'error' must not take the
 * process down. Here we drive R2Store at a local http server that abruptly
 * destroys connections, and assert it retries / surfaces a catchable error
 * rather than throwing asynchronously.
 */

function makeStore(url: string): R2Store {
  return new R2Store({
    endpoint: url,
    bucket: "b",
    accessKeyId: "k",
    secretAccessKey: "s",
    region: "auto",
    keyPrefix: "p",
  });
}

test("get(): retries past a mid-flight connection reset and then succeeds", async () => {
  let hits = 0;
  const server = http.createServer((req, res) => {
    hits++;
    if (hits === 1) {
      // Destroy the socket without responding — mimics a dead keep-alive socket.
      req.socket.destroy();
      return;
    }
    res.writeHead(200, { "content-length": "2" }).end("ok");
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", () => r()));
  const { port } = server.address() as AddressInfo;
  try {
    const store = makeStore(`http://127.0.0.1:${port}`);
    const got = await store.get("x"); // must not crash; must retry to success
    assert.ok(got);
    assert.equal(got.toString(), "ok");
    assert.ok(hits >= 2, "expected at least one retry after the reset");
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});

test("get(): a server that always resets surfaces a catchable rejection", async () => {
  const server = http.createServer((req) => req.socket.destroy());
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", () => r()));
  const { port } = server.address() as AddressInfo;
  try {
    const store = makeStore(`http://127.0.0.1:${port}`);
    await assert.rejects(() => store.get("x"));
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});
