import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";

import { createAnthropicProxy } from "./anthropic-proxy.mjs";

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  return `http://127.0.0.1:${address.port}`;
}

async function close(server) {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}

test("forwards an allowed request with only the real credential", async () => {
  let received;
  const upstream = http.createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) {
      chunks.push(chunk);
    }
    received = {
      authorization: request.headers.authorization,
      body: Buffer.concat(chunks).toString("utf8"),
      key: request.headers["x-api-key"],
      path: request.url,
    };
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ type: "message" }));
  });
  const upstreamUrl = await listen(upstream);
  const proxy = createAnthropicProxy({
    apiKey: "real-secret",
    token: "temporary-token",
    allowedModels: new Set(["claude-test"]),
    upstream: new URL(upstreamUrl),
  });
  const proxyUrl = await listen(proxy);

  try {
    const body = JSON.stringify({ model: "claude-test", max_tokens: 1024, messages: [] });
    const response = await fetch(`${proxyUrl}/v1/messages?beta=true`, {
      method: "POST",
      headers: {
        authorization: "Bearer temporary-token",
        "content-type": "application/json",
      },
      body,
    });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { type: "message" });
    assert.deepEqual(received, {
      authorization: undefined,
      body,
      key: "real-secret",
      path: "/v1/messages?beta=true",
    });
  } finally {
    await close(proxy);
    await close(upstream);
  }
});

test("rejects unauthorized paths, models, and excess requests", async () => {
  let upstreamRequests = 0;
  const upstream = http.createServer((_request, response) => {
    upstreamRequests += 1;
    response.end("ok");
  });
  const upstreamUrl = await listen(upstream);
  const proxy = createAnthropicProxy({
    apiKey: "real-secret",
    token: "temporary-token",
    allowedModels: new Set(["claude-test"]),
    maxRequests: 1,
    upstream: new URL(upstreamUrl),
  });
  const proxyUrl = await listen(proxy);
  const request = (path, model, token = "temporary-token") => fetch(`${proxyUrl}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({ model, max_tokens: 1024 }),
  });

  try {
    assert.equal((await request("/v1/messages", "claude-test", "wrong")).status, 401);
    assert.equal((await request("/v1/complete", "claude-test")).status, 404);
    assert.equal((await request("/v1/messages", "claude-other")).status, 400);
    assert.equal((await request("/v1/messages", "claude-test")).status, 200);
    assert.equal((await request("/v1/messages", "claude-test")).status, 429);
    assert.equal(upstreamRequests, 1);
  } finally {
    await close(proxy);
    await close(upstream);
  }
});
