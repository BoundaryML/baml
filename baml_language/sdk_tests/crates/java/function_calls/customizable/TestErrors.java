// BamlError / BamlPanic delivery contract (31b-phase1 analog).
//
// Port of python_pydantic2/function_calls/customizable/test_errors.py.
//
// A thrown BAML value surfaces in Java as an unchecked `BamlError` (or
// `BamlPanic`) wrapper carrying the decoded value via `.value()` — a plain
// generated value class — plus `.class_name()` and `.baml_trace()` accessors
// (ref-java-codegen-conventions.md; the runtime package `baml_bridge` is
// provisional).
//
// Namespace note: `throws_test.Fns` and `raises_test.Fns` are distinct `Fns`
// holders that cannot both be imported (no import aliases), so `Fns` is fully
// qualified at every call site here.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlCallContext;
import baml_bridge.BamlCancelledError;
import baml_bridge.BamlError;
import baml_bridge.BamlPanic;
import baml_sdk.baml.panics.UserPanic;
import baml_sdk.raises_test.ParseError;
import baml_sdk.throws_test.MyError;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class TestErrors {

    private static final String BAD_JSON = "{not valid json";

    // `File "<src>", line N, in <fn>` — the wire trace-line shape.
    private static final Pattern TRACE_LINE =
            Pattern.compile("File \"(?<file>[^\"]*)\", line (?<line>\\d+), in (?<func>[^\"]+)");

    @Test
    void test_errors_stdlib_error_surfaces_as_baml_error() {
        // `baml.json.parse` on bad input -> BamlError whose `.value()` is a
        // `baml.json.ParseError` (a plain generated value class). Proves stdlib error
        // classes surface structured, independent of any `throws` clause.
        BamlError exc =
                assertThrows(BamlError.class, () -> baml_sdk.throws_test.Fns.ParseJson(BAD_JSON));
        assertInstanceOf(baml_sdk.baml.json.ParseError.class, exc.value());
    }

    @Test
    void test_errors_user_throw_surfaces_declared_instance() {
        // A user `throw` of a declared error -> BamlError whose `.value()` is
        // the declared user error instance itself.
        BamlError exc =
                assertThrows(BamlError.class, () -> baml_sdk.throws_test.Fns.ThrowMyError());
        assertInstanceOf(MyError.class, exc.value());
    }

    @Test
    void test_errors_union_throws_preserves_class_name() {
        // A throw escaping a *multi-member* `throws` union must carry the thrown
        // value's class FQN in `class_name()`, exactly like a single-member
        // throws. `Reparse` declares `throws ParseError` (single); `LoadDoc`
        // declares `throws ParseError | TimeoutError` (union); both throw
        // `ParseError`, so their `class_name()` must agree.
        BamlError single =
                assertThrows(BamlError.class, () -> baml_sdk.raises_test.Fns.Reparse("x"));
        BamlError union =
                assertThrows(BamlError.class, () -> baml_sdk.raises_test.Fns.LoadDoc("x"));

        assertEquals("user.raises_test.ParseError", single.class_name());
        assertEquals(single.class_name(), union.class_name());
        assertInstanceOf(ParseError.class, union.value());
    }

    @Disabled(
            "java-port note: Python triggers a host-side invalid argument by "
                    + "passing an undeclared kwarg (`hello_world(not_a_param=2)`), which "
                    + "bypasses static typing and surfaces at runtime as a BamlError "
                    + "wrapping baml.errors.InvalidArgument. Java is nominally typed: an "
                    + "undeclared argument is a COMPILE error, not a runtime one, so this "
                    + "specific host-synthesized InvalidArgument path has no runtime "
                    + "call-site analog. (The encode-time IllegalArgumentException path is "
                    + "covered by TestGenericInference.test_host_only_object_not_encodable.)")
    @Test
    void test_errors_host_invalid_argument_wraps_baml_errors_invalid_argument() {
        // Intentionally empty — see @Disabled reason.
    }

    @Test
    void test_errors_user_panic_surfaces_as_baml_panic() {
        // A user-initiated panic via `baml.sys.panic` -> BamlPanic whose
        // `.value()` is a `baml.panics.UserPanic`.
        BamlPanic exc =
                assertThrows(
                        BamlPanic.class,
                        () -> baml_sdk.throws_test.Fns.DoPanic("user-initiated boom"));
        assertInstanceOf(UserPanic.class, exc.value());
    }

    @Test
    void test_errors_cancellation_surfaces_as_baml_panic() {
        // java-port note: Python drives the low-level `call_function` +
        // `asyncio.gather`, aborting after ~100ms, and asserts an
        // `asyncio.CancelledError` whose `.reason` is a `BamlCancelledError`.
        // Java uses the generated `SleepMs_async(ms, ctx)` sibling; engine-driven
        // cancellation completes the future exceptionally with a
        // `BamlCancelledError`. Under Design B that type extends
        // `CancellationException`, so `join()` surfaces it DIRECTLY (unwrapped),
        // the analog of Python's directly-raised `CancelledError`.
        BamlCallContext ctx = new BamlCallContext();
        CompletableFuture<Void> future = baml_sdk.throws_test.Fns.SleepMs_async(2000L, ctx);
        CompletableFuture.runAsync(
                () -> {
                    try {
                        Thread.sleep(100);
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                    }
                    ctx.abort();
                });

        assertThrows(BamlCancelledError.class, future::join);
    }

    @Test
    void test_errors_str_is_non_empty() {
        // `toString()` is non-empty — guards the telemetry path, which records it.
        BamlError exc =
                assertThrows(BamlError.class, () -> baml_sdk.throws_test.Fns.ParseJson(BAD_JSON));
        assertFalse(exc.toString().isEmpty());
    }

    @Test
    void test_errors_baml_error_carries_baml_trace() {
        // `.baml_trace()` is the list of rendered BAML stack frames (one per
        // frame), pointing into the throwing function's `.baml` source.
        BamlError exc =
                assertThrows(BamlError.class, () -> baml_sdk.throws_test.Fns.ThrowMyError());
        List<String> trace = exc.baml_trace();
        assertNotNull(trace);
        assertFalse(trace.isEmpty(), "expected a non-empty list");

        // Most-recent-call-last: the throwing function is the last frame.
        Matcher m = TRACE_LINE.matcher(trace.get(trace.size() - 1));
        assertTrue(m.matches(), "trace line not in `File ..., line N, in fn` form");
        assertTrue(m.group("file").endsWith("types.baml"), m.group("file"));
        assertEquals("user.throws_test.ThrowMyError", m.group("func"));
        assertTrue(Integer.parseInt(m.group("line")) >= 1);
    }

    @Test
    void test_errors_baml_trace_spliced_into_python_traceback() {
        // java-port note: Python splices the BAML frames into the exception's
        // real Python traceback so `traceback.format_exception` renders the
        // `.baml` source frame inline. Java renders stack frames as
        // `StackTraceElement`s (`at fn(src:N)`), not the `File "...", line N, in
        // fn` wire form, so the exact wire lines can't appear verbatim. The
        // portable contract asserted here: the wire trace exists AND the
        // rendered native stack trace names the throwing BAML function and its
        // `.baml` source (i.e. the BAML frame is spliced in, not detached).
        BamlError exc =
                assertThrows(BamlError.class, () -> baml_sdk.throws_test.Fns.ParseJson(BAD_JSON));

        assertFalse(exc.baml_trace().isEmpty());

        StringWriter sw = new StringWriter();
        exc.printStackTrace(new PrintWriter(sw));
        String rendered = sw.toString();

        assertTrue(rendered.contains("types.baml"), rendered);
        assertTrue(rendered.contains("user.throws_test.ParseJson"), rendered);
    }

    // Clean exit — subprocess, outside JUnit's caught-exception machinery.
    // `baml.sys.exit(code)` must terminate the process with `code` (via
    // `Runtime.getRuntime().halt(code)` per the completeness doc), NOT throw a
    // catchable BamlPanic. Both a non-zero code and exit(0) are covered: zero is
    // exactly what the `is_exit_panic` discriminator protects.
    @Test
    void test_errors_clean_exit_terminates_process_with_code_0() throws Exception {
        assertCleanExitTerminatesProcessWithCode(0);
    }

    @Test
    void test_errors_clean_exit_terminates_process_with_code_7() throws Exception {
        assertCleanExitTerminatesProcessWithCode(7);
    }

    private void assertCleanExitTerminatesProcessWithCode(int code) throws Exception {
        String javaBin = System.getProperty("java.home") + "/bin/java";
        String classpath = System.getProperty("java.class.path");

        Process process =
                new ProcessBuilder(
                                javaBin,
                                "-cp",
                                classpath,
                                "TestErrors$ExitSnippet",
                                String.valueOf(code))
                        .start();

        String stdout = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        int returnCode = process.waitFor();

        assertEquals(code, returnCode, "stdout=" + stdout);
        assertFalse(stdout.contains("UNREACHABLE"));
    }

    // Subprocess entry point for the clean-exit tests.
    // `DoExit(code)` hard-exits before the print, mirroring Python's os._exit
    // snippet. Launched via `java -cp <cp> TestErrors$ExitSnippet <code>`.
    public static final class ExitSnippet {
        public static void main(String[] args) {
            long code = Long.parseLong(args[0]);
            baml_sdk.throws_test.Fns.DoExit(code);
            System.out.println("UNREACHABLE"); // halt must fire before this
        }
    }
}
