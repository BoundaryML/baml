// Coverage for handle-backed stdlib types returned from BAML to Java.
//
// The non-media cases are encode-back tests: Java receives a generated
// class instance with an embedded handle, calls generated stdlib methods
// with that same instance, and the engine must see the original handle
// state. No external dependency: the HTTP test binds an ephemeral
// localhost server (JDK's built-in `com.sun.net.httpserver.HttpServer`)
// and the FS test uses a temp file.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_handles.py — same test names, cases, inputs, assertions.
//
// java-port note: pytest's `http_server` / `temp_file` fixtures have no
// direct JUnit 5 analog used elsewhere in this port; each test that needs
// one sets it up and tears it down inline (try/finally) instead, since the
// resource (ephemeral port / temp dir) is local to a single test method.
//
// java-port note: `baml.fs.Fns` and `baml.http.Fns` are two distinct
// generated holder classes that are both literally named `Fns` (one per
// namespace, per the conventions doc), so at most one can be imported by
// simple name in a single file — the other is referenced fully qualified.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_sdk.baml.fs.File;
import baml_sdk.baml.fs.Fns;
import baml_sdk.baml.http.Response;
import baml_sdk.baml.media.Image;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;

class TestHandles {

    // 1x1 transparent PNG.
    private static final String PNG_B64 =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
                    + "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC";

    // --- media: Image.from_base64 -------------------------------------------

    @Test
    void test_handles_image_from_base64_roundtrips_payload() {
        Image img = Image.from_base64(PNG_B64, "image/png");
        assertEquals("image/png", img.mime_type());
        assertEquals(PNG_B64, img.base64());
    }

    // --- baml.http.Response --------------------------------------------------

    private static final byte[] HTTP_BODY =
            "hello from localhost".getBytes(StandardCharsets.UTF_8);

    @Test
    void test_handles_http_get_response_fields_and_methods() throws IOException {
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext(
                "/",
                exchange -> {
                    exchange.getResponseHeaders().add("Content-Type", "text/plain");
                    exchange.sendResponseHeaders(200, HTTP_BODY.length);
                    try (OutputStream os = exchange.getResponseBody()) {
                        os.write(HTTP_BODY);
                    }
                });
        server.start();
        try {
            String url = "http://127.0.0.1:" + server.getAddress().getPort() + "/";
            Response resp = baml_sdk.baml.http.Fns.fetch(url);
            assertEquals(200L, resp.status_code());
            assertTrue(resp.ok());
            assertEquals(new String(HTTP_BODY, StandardCharsets.UTF_8), resp.text());
        } finally {
            server.stop(0);
        }
    }

    // --- baml.fs.File: cursor state preserved across calls --------------------

    @Test
    void test_handles_open_file_returns_file_handle() throws IOException {
        Path dir = Files.createTempDirectory("baml-handles-test");
        Path path = dir.resolve("digits.txt");
        Files.writeString(path, "0123456789");
        try {
            File f = Fns.open(path.toString(), "r");
            // java-port note: Python asserts `type(f).__name__ == "File"`;
            // the Java analog is the static type itself — `open`'s declared
            // return type is already `baml_sdk.baml.fs.File`, so the runtime
            // type is pinned at compile time and needs no separate check.
            assertNull(f.close());
        } finally {
            Files.deleteIfExists(path);
            Files.deleteIfExists(dir);
        }
    }

    @Test
    void test_handles_file_cursor_state_persists_across_calls() throws IOException {
        Path dir = Files.createTempDirectory("baml-handles-test");
        Path path = dir.resolve("digits.txt");
        Files.writeString(path, "0123456789");
        // Hoisted so a failing assertion below cannot leak the open engine-side
        // file handle: it is closed in `finally` (before the deletes, which on
        // some platforms require the file be closed first).
        File f = null;
        try {
            f = Fns.open(path.toString(), "r");

            // Two successive relative seeks on the *same* handle must
            // accumulate — the second continues where the first stopped.
            // This is the load-bearing assertion: engine-side file state
            // survives across separate host->engine FFI calls. (`read`
            // reaches the cursor too, but it comes from `baml.io.Read`, and
            // interface methods are not on the bridge surface.)
            assertEquals(3L, f.seek_from("current", 3L));
            assertEquals(6L, f.seek_from("current", 3L));

            // Seek back to the start and confirm the cursor actually moved.
            assertEquals(0L, f.seek_from("start", 0L));
            assertEquals(2L, f.seek_from("current", 2L));

            // text() reads from the current cursor (now at 2) to EOF.
            assertEquals("23456789", f.text());

            assertNull(f.close());
            f = null; // closed cleanly; skip the finally-close.
        } finally {
            if (f != null) {
                f.close();
            }
            Files.deleteIfExists(path);
            Files.deleteIfExists(dir);
        }
    }
}
