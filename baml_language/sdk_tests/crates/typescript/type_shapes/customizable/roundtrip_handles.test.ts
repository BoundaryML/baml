// Coverage for handle-backed stdlib types returned from BAML. The non-media
// cases are intentionally encode-back tests through user types that wrap
// stdlib handles (`stdlib_wrappers.baml`): the host receives a user class with
// an embedded BamlHandle, passes that same instance back to user functions,
// and the engine must see the original handle state.
import {
  HttpExchange,
  OpenFile,
  baml,
  close_open_file,
  fetch_http_exchange_async,
  http_exchange_text,
  http_exchange_text_async,
  make_http_exchange,
  open_read_only,
  read_open_file,
  seek_open_file,
} from "./baml_sdk/index.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { isTestRuntime } from "./test_runtime.js";

let http: typeof import("node:http");
let fs: typeof import("node:fs");
let os: typeof import("node:os");
let path: typeof import("node:path");
if (isTestRuntime("node")) {
  http = await import("node:http");
  fs = await import("node:fs");
  os = await import("node:os");
  path = await import("node:path");
}

// 1x1 transparent PNG.
const PNG_B64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk" +
  "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC";

describe("roundtrip handles — media Image.fromBase64", () => {
  it("handles_image_from_base64_roundtrips_payload", () => {
    const img = baml.media.Image.fromBase64(PNG_B64, "image/png");
    expect(img.mimeType()).toBe("image/png");
    expect(img.base64()).toBe(PNG_B64);
  });
});

// Constructing a `baml.http.Response` is a host capability that browsers and Workers do not provide.
describe.runIf(isTestRuntime("node"))("roundtrip handles — user type wrapping baml.http.Response", () => {
  // SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
  it("handles_user_type_wraps_http_response", () => {
    const exchange = make_http_exchange("wrapped", "hello from BAML");
    expect(exchange).toBeInstanceOf(HttpExchange);
    expect(exchange.label).toBe("wrapped");
    expect(exchange.response.status_code).toBe(200);
    expect(exchange.response.headers["x-label"]).toBe("wrapped");
    expect(http_exchange_text(exchange)).toBe("hello from BAML");
  });
});

// This fixture owns a local node:http listener, which is not a browser or Workers capability.
describe.runIf(isTestRuntime("node"))(
  "roundtrip handles — user type wrapping a fetched baml.http.Response",
  () => {
    const HTTP_BODY = "hello from localhost";
    let server: import("node:http").Server;
    let url: string;

    beforeAll(async () => {
      server = http.createServer((_req, res) => {
        res.writeHead(200, {
          "Content-Type": "text/plain",
          "Content-Length": String(Buffer.byteLength(HTTP_BODY)),
        });
        res.end(HTTP_BODY);
      });
      await new Promise<void>((resolve) =>
        server.listen(0, "127.0.0.1", resolve),
      );
      const addr = server.address();
      const port = typeof addr === "object" && addr ? addr.port : 0;
      url = `http://127.0.0.1:${port}/`;
    });

    afterAll(async () => {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    });

    // SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
    it("handles_user_type_wraps_fetched_http_response", async () => {
      // Must be async: the sync path blocks the Node main thread, starving the
      // libuv loop the localhost server runs on (Python runs it in a thread).
      const exchange = await fetch_http_exchange_async(url);
      expect(exchange.label).toBe(url);
      expect(exchange.response.status_code).toBe(200);
      expect(await http_exchange_text_async(exchange)).toBe(HTTP_BODY);
    });
  },
);

// These cases create and mutate temporary host files through Node filesystem APIs.
describe.runIf(isTestRuntime("node"))(
  "roundtrip handles — user type wrapping baml.fs.File",
  () => {
    let dir: string;
    let filePath: string;

    beforeAll(() => {
      dir = fs.mkdtempSync(path.join(os.tmpdir(), "baml-handles-"));
      filePath = path.join(dir, "digits.txt");
      fs.writeFileSync(filePath, "0123456789");
    });

    afterAll(() => {
      fs.rmSync(dir, { recursive: true, force: true });
    });

    // SDK_PARITY_LINT(skip): stdlib handles reach the host only through user types in SDKs that omit stdlib functions
    it("handles_user_type_wraps_file_handle", () => {
      const opened = open_read_only(filePath);
      expect(opened).toBeInstanceOf(OpenFile);
      expect(opened.path).toBe(filePath);
      expect(opened.file.constructor.name).toBe("File");
      expect(close_open_file(opened)).toBeNull();
    });

    it("handles_file_cursor_state_persists_across_calls", () => {
      const opened = open_read_only(filePath);

      // Relative seeks verify that separate calls share one engine-side handle.
      expect(seek_open_file(opened, "current", 3)).toBe(3);
      expect(seek_open_file(opened, "current", 3)).toBe(6);
      expect(seek_open_file(opened, "start", 0)).toBe(0);
      expect(seek_open_file(opened, "current", 2)).toBe(2);
      expect(read_open_file(opened)).toBe("23456789");
      expect(close_open_file(opened)).toBeNull();
    });
  },
);
