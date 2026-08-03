// Coverage for handle-backed stdlib types returned from BAML. The non-media
// cases are intentionally encode-back tests: the host receives a generated
// class instance with an embedded BamlHandle, calls generated stdlib methods
// with that same instance, and the engine must see the original handle state.
import { baml } from "./baml_sdk/index.js";
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

// This fixture owns a local node:http listener, which is not a browser or Workers capability.
describe.runIf(isTestRuntime("node"))(
  "roundtrip handles — baml.http.Response",
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

    it("handles_http_get_response_fields_and_methods", async () => {
      // Must be async: the sync path blocks the Node main thread, starving the
      // libuv loop the localhost server runs on (Python runs it in a thread).
      const resp = await baml.http.fetch_async(url);
      expect(resp.status_code).toBe(200);
      expect(resp.text()).toBe(HTTP_BODY);
    });
  },
);

// These cases create and mutate temporary host files through Node filesystem APIs.
describe.runIf(isTestRuntime("node"))(
  "roundtrip handles — baml.fs.File",
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

    it("handles_baml_fs_open_returns_a_typed_file_handle", () => {
      const f = baml.fs.open(filePath, "r");
      expect(f).toBeDefined();
      expect(f.constructor.name).toBe("File");
      expect(f.close()).toBeNull();
    });

    it("handles_file_cursor_state_persists_across_calls", () => {
      const f = baml.fs.open(filePath, "r");

      expect(f.read(3)).toBe("012");
      expect(f.read(3)).toBe("345");
      expect(f.seek_from("start", 0)).toBe(0);
      expect(f.read(2)).toBe("01");
      expect(f.text()).toBe("23456789");
      expect(f.close()).toBeNull();
    });
  },
);
