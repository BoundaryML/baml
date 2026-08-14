import "./baml_sdk/index.js";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  exists_async,
  open_async,
  read,
  read_async,
} from "./baml_sdk/baml/fs/index.js";
import {
  Request,
  _fetch,
  _fetch_async,
  _send_async,
  fetch_sse_async,
} from "./baml_sdk/baml/http/index.js";
import { get_async } from "./baml_sdk/baml/env/index.js";
import { isTestRuntime } from "./test_runtime.js";

const BUNDLE_FILE = "/bundle/index.mjs";
const BUNDLE_MARKER = "cloudflare:test-internal";
const isWebRuntime = isTestRuntime("web") || isTestRuntime("workers");

afterEach(() => {
  vi.restoreAllMocks();
});

describe.runIf(isWebRuntime)("Web fetch sysops", () => {
  it("web_sysops_trampolines_baml_http_fetch_to_global_fetch_and_buffers_the_response", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("hello from fetch", {
        status: 201,
        headers: { "x-web-test": "fetch" },
      }),
    );

    const response = await _fetch_async("https://example.test/fetch", 0n);

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(response.status_code).toBe(201);
    expect(response.headers["x-web-test"]).toBe("fetch");
    await expect(response.text_async()).resolves.toBe("hello from fetch");
    await expect(response.bytes_async()).rejects.toThrow(
      /consumed|Io|Invalid handle/i,
    );
  });

  it("web_sysops_trampolines_baml_http_send_with_method_headers_and_body", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        new Response(Uint8Array.from([1, 2, 3]), { status: 202 }),
      );
    const request = new Request({
      method: "POST",
      url: "https://example.test/send",
      headers: {
        "content-type": "application/octet-stream",
        "x-web-test": "send",
      },
      body: "payload",
    });

    const response = await _send_async(request, 0n);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [, init] = fetchMock.mock.calls[0];
    expect(init?.method).toBe("POST");
    expect(new Headers(init?.headers).get("x-web-test")).toBe("send");
    expect(new TextDecoder().decode(init?.body as Uint8Array)).toBe("payload");
    await expect(response.bytes_async()).resolves.toEqual(
      Uint8Array.from([1, 2, 3]),
    );
  });

  it("web_sysops_maps_fetch_failures_and_timeouts_into_declared_baml_errors", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValueOnce(
      new Error("network unavailable"),
    );
    await expect(_fetch_async("https://example.test/io", 0n)).rejects.toThrow(
      /network unavailable|Io/i,
    );

    vi.spyOn(globalThis, "fetch").mockImplementationOnce(
      ((_input: unknown, init?: { signal?: AbortSignal }) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          );
        })) as typeof globalThis.fetch,
    );
    await expect(
      _fetch_async("https://example.test/timeout", 1_000_000n),
    ).rejects.toThrow(/timeout/i);
  });
});

// Only the workerd package installs the synchronous bundle filesystem adapter.
describe.runIf(isTestRuntime("workers"))("Workers fs.readFileSync sysop", () => {
  it("web_sysops_supports_sync_and_async_baml_fs_read_through_node_fs_read_file_sync", async () => {
    expect(read(BUNDLE_FILE)).toContain(BUNDLE_MARKER);
    await expect(read_async(BUNDLE_FILE)).resolves.toContain(BUNDLE_MARKER);
  });
});

// Browsers do not expose the workerd bundle filesystem adapter.
describe.runIf(isTestRuntime("web"))("Browser filesystem capability boundary", () => {
  it("web_sysops_rejects_sync_and_async_baml_fs_read_promptly", async () => {
    expect(() => read(BUNDLE_FILE)).toThrow();
    await expect(read_async(BUNDLE_FILE)).rejects.toThrow();
  });
});

describe.runIf(isWebRuntime)("Web capability boundary", () => {
  it("web_sysops_rejects_sync_http_before_dispatching_fetch", { timeout: 2_000 }, () => {
    const fetchMock = vi.spyOn(globalThis, "fetch");
    expect(() => _fetch("https://example.test/sync", 0n)).toThrow(/callFunctionSync|async API/i);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("web_sysops_rejects_unsupported_filesystem_operations", async () => {
    await expect(exists_async(BUNDLE_FILE)).rejects.toThrow();
    await expect(open_async(BUNDLE_FILE, "r")).rejects.toThrow();
  });

  it("web_sysops_rejects_http_streaming_and_unrelated_sysops", async () => {
    const request = new Request({
      method: "GET",
      url: "https://example.test/sse",
      headers: {},
      body: "",
    });
    await expect(fetch_sse_async(request)).rejects.toThrow();
    await expect(get_async("SHOULD_NOT_BE_VISIBLE")).rejects.toThrow();
  });
});
