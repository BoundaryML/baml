// Keyless replay harness shared by the streaming tests.
//
// Port of python_pydantic2/llm_functions/customizable/replay_harness.py.
//
// The Python original is a pytest helper (NOT a test): the
// `@replay_server(recording_path=...)` decorator runs the wrapped test against
// an in-process BAML server replaying a checked-in SSE recording, with the
// env-driven `StreamStub` client pointed at it — so the streaming path is
// exercised with **no OPENAI_API_KEY**. Inside a decorated test the server
// address is exposed via `os.environ["BAML_REPLAY_BASE_URL"]`.
//
// JUnit has no decorator/context-manager analog, so this ports to an
// `AutoCloseable` helper used with try-with-resources — the direct analog of
// Python's `with _running_server(recording): ...`:
//
//     try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
//         ... drive the stream via the StreamStub client ...
//     }
//
// The server itself is the BAML-implemented replay server in the `replay`
// namespace (ns_replay/replay_server.baml): `replay_serve_until_shutdown` binds
// an ephemeral port, writes the bound address to `addr_file` (we poll for it),
// serves the recording until a `/__replay__/shutdown` POST, then returns.

import baml_bridge.BridgeEnv;
import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

final class ReplayHarness implements AutoCloseable {

    private final String addr;
    private final Thread serverThread;
    private final Path addrFile;

    private ReplayHarness(String addr, Thread serverThread, Path addrFile) {
        this.addr = addr;
        this.serverThread = serverThread;
        this.addrFile = addrFile;
    }

    /** Absolute path to a checked-in SSE recording under sdk_tests/fixtures.
     *  Java analog of Python's `recording_path(name)`: walk up from the working
     *  directory (Gradle runs the `test` task from the fixture `generated/`
     *  root) to the `sdk_tests/` ancestor, then resolve the recording. */
    static Path recording_path(String name) {
        Path start = Path.of("").toAbsolutePath();
        for (Path p = start; p != null; p = p.getParent()) {
            Path fileName = p.getFileName();
            if (fileName != null && fileName.toString().equals("sdk_tests")) {
                Path rec =
                        p.resolve("fixtures")
                                .resolve("llm_functions")
                                .resolve("recordings")
                                .resolve(name + ".snap.sse");
                if (!Files.exists(rec)) {
                    throw new IllegalStateException("missing recording " + rec);
                }
                return rec;
            }
        }
        throw new IllegalStateException("could not locate the sdk_tests/ ancestor directory");
    }

    /** Serve `recording` on a background thread, point the replay client env at
     *  it, and return a handle whose `close()` tears the server down. */
    static ReplayHarness start(String recording) {
        Path rec = recording_path(recording);
        Path addrFile =
                Path.of(
                        System.getProperty("java.io.tmpdir"),
                        "baml_replay_"
                                + ProcessHandle.current().pid()
                                + "_"
                                + Thread.currentThread().getId()
                                + "_"
                                + recording);
        try {
            Files.deleteIfExists(addrFile);
        } catch (IOException ignored) {
            // best effort
        }

        List<Throwable> error = Collections.synchronizedList(new ArrayList<>());
        Thread thread =
                new Thread(
                        () -> {
                            try {
                                // Blocking serve; returns the bound address on shutdown.
                                baml_sdk.replay.Fns.replay_serve_until_shutdown(
                                        rec.toString(), addrFile.toString());
                            } catch (Throwable t) {
                                // Surface bridge/engine failures to the poller below.
                                error.add(t);
                            }
                        });
        thread.setDaemon(true);
        thread.start();

        String addr = null;
        long deadline = System.nanoTime() + Duration.ofSeconds(10).toNanos();
        while (System.nanoTime() < deadline) {
            if (!error.isEmpty()) {
                throw new RuntimeException("replay server thread failed", error.get(0));
            }
            try {
                if (Files.exists(addrFile)) {
                    String text = Files.readString(addrFile).strip();
                    if (!text.isEmpty()) {
                        addr = text;
                        break;
                    }
                }
            } catch (IOException ignored) {
                // file may be mid-write; retry
            }
            try {
                Thread.sleep(20);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new RuntimeException(e);
            }
        }
        if (addr == null) {
            throw new RuntimeException("replay server did not bind within 10s");
        }

        // java-port TODO: env-var plumbing to the engine.
        // The StreamStub client resolves `base_url`/`api_key` from
        // BAML_REPLAY_BASE_URL / BAML_REPLAY_API_KEY **at call time**, via the
        // engine's `std::env`. Python mutates `os.environ`, which the pyo3
        // in-process engine reads directly. Java has NO portable runtime
        // setenv, and JUnit-Pioneer / reflection patch only the JVM's cached
        // env view — NOT the native `getenv` the JNI-linked engine reads. So
        // this must route through a native setenv exposed by `bridge_java`.
        // `BridgeEnv` is a placeholder for that native hook (see invented-API
        // notes); swap it for the real bridge call once it lands.
        BridgeEnv.set("BAML_REPLAY_BASE_URL", "http://" + addr);
        BridgeEnv.set("BAML_REPLAY_API_KEY", "replay-test-key");

        return new ReplayHarness(addr, thread, addrFile);
    }

    /** The bound `host:port` of the running replay server. */
    String address() {
        return addr;
    }

    @Override
    public void close() {
        // Ask the server to shut down (mirrors the `/__replay__/shutdown` POST).
        try {
            HttpClient.newHttpClient()
                    .send(
                            HttpRequest.newBuilder(
                                            URI.create("http://" + addr + "/__replay__/shutdown"))
                                    .POST(HttpRequest.BodyPublishers.noBody())
                                    .timeout(Duration.ofSeconds(5))
                                    .build(),
                            HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {
            // best effort — the server is a daemon thread regardless
        }
        try {
            serverThread.join(Duration.ofSeconds(10).toMillis());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        try {
            Files.deleteIfExists(addrFile);
        } catch (IOException ignored) {
            // best effort
        }
        // java-port TODO: same native-setenv caveat as in start().
        BridgeEnv.unset("BAML_REPLAY_BASE_URL");
        BridgeEnv.unset("BAML_REPLAY_API_KEY");
    }
}
