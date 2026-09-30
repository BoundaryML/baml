// Web bridge sysop coverage. Generated SDKs do not expose stdlib functions, so
// each case reaches the sysop through a user function in
// `stdlib_wrappers.baml` that calls the stdlib from BAML.
import {
  env_var_async,
  fetch_text,
  fetch_text_async,
  fetch_text_then_bytes_async,
  fetch_text_with_timeout_async,
  open_and_read_async,
  open_sse_async,
  path_exists_async,
  read_text_file,
  read_text_file_async,
  send_for_bytes_async,
} from "./baml_sdk/index.js";
import { afterEach, describe, expect, it, vi } from "vitest";
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

    const fetched = await fetch_text_async("https://example.test/fetch");

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(fetched.status_code).toBe(201);
    expect(fetched.headers["x-web-test"]).toBe("fetch");
    expect(fetched.text).toBe("hello from fetch");

    // A buffered body is consumed by its first read.
    fetchMock.mockResolvedValueOnce(new Response("hello again"));
    await expect(
      fetch_text_then_bytes_async("https://example.test/fetch"),
    ).rejects.toThrow(/consumed|Io|Invalid handle/i);
  });

  it("web_sysops_trampolines_baml_http_send_with_method_headers_and_body", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        new Response(Uint8Array.from([1, 2, 3]), { status: 202 }),
      );
    const bytes = await send_for_bytes_async(
      "POST",
      "https://example.test/send",
      {
        "content-type": "application/octet-stream",
        "x-web-test": "send",
      },
      "payload",
    );

    expect(fetchMock).toHaveBeenCalledOnce();
    const [, init] = fetchMock.mock.calls[0];
    expect(init?.method).toBe("POST");
    expect(new Headers(init?.headers).get("x-web-test")).toBe("send");
    expect(new TextDecoder().decode(init?.body as Uint8Array)).toBe("payload");
    expect(bytes).toEqual(Uint8Array.from([1, 2, 3]));
  });

  it("web_sysops_maps_fetch_failures_and_timeouts_into_declared_baml_errors", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValueOnce(
      new Error("network unavailable"),
    );
    await expect(fetch_text_async("https://example.test/io")).rejects.toThrow(
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
      fetch_text_with_timeout_async("https://example.test/timeout", 1_000_000n),
    ).rejects.toThrow(/timeout/i);
  });
});

// Only the workerd package installs the synchronous bundle filesystem adapter.
describe.runIf(isTestRuntime("workers"))("Workers fs.readFileSync sysop", () => {
  it("web_sysops_supports_sync_and_async_baml_fs_read_through_node_fs_read_file_sync", async () => {
    expect(read_text_file(BUNDLE_FILE)).toContain(BUNDLE_MARKER);
    await expect(read_text_file_async(BUNDLE_FILE)).resolves.toContain(BUNDLE_MARKER);
  });
});

// Browsers do not expose the workerd bundle filesystem adapter.
describe.runIf(isTestRuntime("web"))("Browser filesystem capability boundary", () => {
  it("web_sysops_rejects_sync_and_async_baml_fs_read_promptly", async () => {
    expect(() => read_text_file(BUNDLE_FILE)).toThrow();
    await expect(read_text_file_async(BUNDLE_FILE)).rejects.toThrow();
  });
});

describe.runIf(isWebRuntime)("Web capability boundary", () => {
  it("web_sysops_rejects_sync_http_before_dispatching_fetch", { timeout: 2_000 }, () => {
    const fetchMock = vi.spyOn(globalThis, "fetch");
    expect(() => fetch_text("https://example.test/sync")).toThrow(/callFunctionSync|async API/i);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("web_sysops_rejects_unsupported_filesystem_operations", async () => {
    await expect(path_exists_async(BUNDLE_FILE)).rejects.toThrow();
    await expect(open_and_read_async(BUNDLE_FILE)).rejects.toThrow();
  });

  it("web_sysops_rejects_http_streaming_and_unrelated_sysops", async () => {
    await expect(open_sse_async("https://example.test/sse")).rejects.toThrow();
    await expect(env_var_async("SHOULD_NOT_BE_VISIBLE")).rejects.toThrow();
  });
});
