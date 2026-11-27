// HTTP requests that BAML sends itself (`baml.http`), against a server on a
// loopback port this file starts. It runs against today's SDK and against
// the native bindings: the same answers, and the same errors.
import "./baml_sdk/index.js";
import { createServer, type Server } from "node:http";
import { createServer as createNetServer } from "node:net";
import type { AddressInfo } from "node:net";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { BamlError } from "@boundaryml/baml-bridge";
import {
  Echo,
  Greeting,
  HttpReply,
  http_get,
  http_post_json,
} from "./baml_sdk/bridge_test/index.js";

let server: Server;
let base = "";

beforeAll(async () => {
  server = createServer((request, response) => {
    const reply = (status: number, body: string) => {
      response.writeHead(status, { "content-length": Buffer.byteLength(body), connection: "close" });
      response.end(body);
    };
    if (request.method === "GET" && request.url === "/hello") {
      reply(200, "hello");
    } else if (request.method === "GET" && request.url?.startsWith("/slow/")) {
      setTimeout(() => reply(200, "slow"), Number(request.url.slice("/slow/".length)));
    } else if (request.method === "POST" && request.url === "/echo") {
      let body = "";
      request.on("data", (chunk) => (body += chunk));
      request.on("end", () => reply(200, JSON.stringify({ ...JSON.parse(body), served_by: "node-test" })));
    } else {
      reply(404, "no");
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => {
  server.close();
});

/** A loopback port nothing listens on. */
const closedPort = async (): Promise<number> => {
  const probe = createNetServer();
  await new Promise<void>((resolve) => probe.listen(0, "127.0.0.1", resolve));
  const { port } = probe.address() as AddressInfo;
  await new Promise<void>((resolve) => probe.close(() => resolve()));
  return port;
};

const failure = async (call: Promise<unknown>): Promise<BamlError> => {
  const error = await call.then(
    () => undefined,
    (e: unknown) => e,
  );
  expect(error).toBeInstanceOf(BamlError);
  return error as BamlError;
};

describe("http parity", () => {
  it("http_one_get", async () => {
    expect(await http_get(`${base}/hello`, 5000)).toEqual(new HttpReply({ status: 200, body: "hello" }));
  });

  it("http_one_post_with_a_json_body_and_a_class_back", async () => {
    const echo = await http_post_json(`${base}/echo`, new Greeting({ name: "Ada", count: 3 }));
    expect(echo).toEqual(new Echo({ name: "Ada", count: 3, served_by: "node-test" }));
  });

  it("http_a_hundred_requests_at_once_overlap", async () => {
    const started = Date.now();
    const replies = await Promise.all(Array.from({ length: 100 }, () => http_get(`${base}/slow/300`, 10_000)));
    const elapsed = Date.now() - started;
    for (const reply of replies) {
      expect(reply).toEqual(new HttpReply({ status: 200, body: "slow" }));
    }
    // One after another they would take 30 s.
    expect(elapsed).toBeLessThan(5000);
  });

  it("http_a_refused_connection", async () => {
    const error = await failure(http_get(`http://127.0.0.1:${await closedPort()}/hello`, 5000));
    expect(error.className).toBe("baml.errors.Io");
  });

  it("http_a_timeout", async () => {
    const started = Date.now();
    const error = await failure(http_get(`${base}/slow/3000`, 200));
    expect(error.className).toBe("baml.errors.Timeout");
    expect(Date.now() - started).toBeLessThan(2000);
  });
});
