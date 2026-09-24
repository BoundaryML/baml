import { afterEach, describe, expect, it, vi } from "vitest";
import { httpApi } from "./api";

interface Call {
  url: string;
  init: RequestInit | undefined;
}

function stubFetch(status: number, body: string): Call[] {
  const calls: Call[] = [];
  vi.stubGlobal("fetch", (url: string, init?: RequestInit) => {
    calls.push({ url, init });
    return Promise.resolve(new Response(body, { status }));
  });
  return calls;
}

afterEach(() => vi.unstubAllGlobals());

describe("httpApi", () => {
  it("returns a run record that has an error field of its own", async () => {
    // `POST /api/runs/:id/kill` on a run without a snapshot answers 200 with a lost run.
    const lost = { id: "r-1", site: "local", status: "lost", error: "the worker process was lost (signal 9)" };
    stubFetch(200, JSON.stringify(lost));
    await expect(httpApi.command("local", "r-1", "kill")).resolves.toEqual(lost);
  });

  it("reports the error field of a 4xx response with the site and the status", async () => {
    stubFetch(409, JSON.stringify({ error: "run r-1 is not durable" }));
    await expect(httpApi.command("local", "r-1", "pause")).rejects.toThrow("local: 409 run r-1 is not durable");
  });

  it("reports a 5xx response that is not JSON", async () => {
    stubFetch(502, "Bad Gateway");
    await expect(httpApi.info("cloud")).rejects.toThrow("cloud: 502 Bad Gateway");
  });

  it("rejects a successful response that is not JSON", async () => {
    stubFetch(200, "<html>");
    await expect(httpApi.info("local")).rejects.toThrow("response is not JSON");
  });

  it("sends each command to the route of section 3.3 on the owning site", async () => {
    const run = JSON.stringify({ id: "r-1", site: "cloud", status: "running" });
    const calls = stubFetch(200, run);
    await httpApi.startRun("local", { function: "durable_plan_trip", args: { city: "Lisbon" } });
    await httpApi.command("cloud", "r-1", "pause");
    await httpApi.command("cloud", "r-1", "resume_here");
    await httpApi.command("cloud", "r-1", "resume_on", { site: "local" });
    await httpApi.command("cloud", "r-1", "resume_on", { site: "cloud2" });
    await httpApi.command("cloud", "r-1", "kill");
    await httpApi.command("cloud", "r-1", "cancel");
    await httpApi.command("cloud", "r-1", "fork");
    await httpApi.command("cloud", "r-1", "fork", { snapshot: 2 });
    expect(calls.map((call) => [call.init?.method, call.url, call.init?.body])).toEqual([
      ["POST", "/local/api/runs", '{"function":"durable_plan_trip","args":{"city":"Lisbon"}}'],
      ["POST", "/cloud/api/runs/r-1/pause", "{}"],
      ["POST", "/cloud/api/runs/r-1/resume", "{}"],
      ["POST", "/cloud/api/runs/r-1/resume", '{"site":"local"}'],
      ["POST", "/cloud/api/runs/r-1/resume", '{"site":"cloud2"}'],
      ["POST", "/cloud/api/runs/r-1/kill", "{}"],
      ["POST", "/cloud/api/runs/r-1/cancel", "{}"],
      ["POST", "/cloud/api/runs/r-1/fork", "{}"],
      ["POST", "/cloud/api/runs/r-1/fork", '{"n":2}'],
    ]);
  });

  it("refuses a resume on another site that names no destination or the run's own site", async () => {
    const calls = stubFetch(200, "{}");
    await expect(httpApi.command("cloud", "r-1", "resume_on")).rejects.toThrow("needs a destination site");
    await expect(httpApi.command("cloud", "r-1", "resume_on", { site: "cloud" })).rejects.toThrow("needs a destination site");
    expect(calls).toHaveLength(0);
  });

  it("sends requests for any site name to the proxy prefix of that site", async () => {
    const calls = stubFetch(200, JSON.stringify({ site: "cloud2", program_dir: "../program", functions: [] }));
    await httpApi.info("cloud2");
    expect(calls[0]?.url).toBe("/cloud2/api/info");
  });

  it("reads queries with GET and fills the site into events that lack it", async () => {
    const calls = stubFetch(
      200,
      JSON.stringify([
        { v: 1, type: "hello", ts: 1, run: "r-1", segment: 1, pid: 7, mode: "start", function: "f", durable: false },
        { type: "from_the_future", ts: 2, run: "r-1" },
      ]),
    );
    const events = await httpApi.runEvents("cloud", "r-1");
    expect(calls[0]?.url).toBe("/cloud/api/runs/r-1/events");
    expect(calls[0]?.init?.method).toBe("GET");
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ type: "hello", site: "cloud" });
  });

  it("encodes the file of a source query", async () => {
    const calls = stubFetch(200, JSON.stringify({ file: "baml_src/a b.baml", text: "" }));
    await httpApi.source("local", "baml_src/a b.baml");
    expect(calls[0]?.url).toBe("/local/api/source?file=baml_src%2Fa%20b.baml");
    await httpApi.snapshotState("local", "r-1", 3);
    expect(calls[1]?.url).toBe("/local/api/runs/r-1/snapshots/3/state");
  });
});
