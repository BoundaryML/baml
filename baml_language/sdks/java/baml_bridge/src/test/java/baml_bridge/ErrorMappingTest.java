package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.BamlTraceback;
import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.WireWriter;

import java.io.PrintWriter;
import java.io.StringWriter;
import java.util.List;

import org.junit.jupiter.api.MethodOrderer;
import org.junit.jupiter.api.Order;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestMethodOrder;

/**
 * Offline error-mapping tests — no native library required. They validate the
 * decided error-mapping slice against the hand-rolled protobuf wire codec:
 *
 * <ul>
 *   <li>a {@code baml.errors.TypeMismatch} thrown value remaps to a native
 *       {@link IllegalArgumentException} carrying the value's {@code message}
 *       field (the analog of the Python bridge's {@code TypeError} remap);
 *   <li>{@link BamlTraceback} synthesizes {@link StackTraceElement}s from the
 *       wire trace lines and prepends them (good lines parsed, malformed lines
 *       skipped, empty trace leaves the stack untouched);
 *   <li>{@link BamlPanic} is an {@link Error}, not an {@link Exception} — a bare
 *       {@code catch (Exception)} does not swallow it.
 * </ul>
 *
 * <p>Method order is pinned so the {@code TypeMismatch}-via-map test runs before
 * the one that registers {@code baml.errors.TypeMismatch} in the process-global
 * {@link TypeRegistry} (registration is sticky — there is no unregister).
 */
@TestMethodOrder(MethodOrderer.OrderAnnotation.class)
class ErrorMappingTest {
    // BamlOutboundValue oneof field numbers (subset used here).
    private static final int OV_STRING = 3;
    private static final int OV_CLASS = 7;

    private static final String TYPE_MISMATCH_FQN = "baml.errors.TypeMismatch";

    /**
     * A generated-shape stand-in for {@code baml.errors.TypeMismatch}: a value
     * class with a single {@code message} field and a PreserveCase {@code
     * message()} accessor, so the remap's reflective {@code message()} probe
     * resolves when the FQN is registered.
     */
    public static final class TypeMismatchLike {
        private final String message;

        public TypeMismatchLike(String message) {
            this.message = message;
        }

        public String message() {
            return message;
        }
    }

    // -- wire builders (mirror the engine's error-arm encode) ----------------

    /**
     * A {@code BamlOutboundResult.error} envelope wrapping a {@code class_value}
     * with the given FQN, a single {@code message} field, and the trace lines.
     */
    private static byte[] errorEnvelope(String classFqn, String message, String... traceLines) {
        WireWriter messageValue = new WireWriter();
        messageValue.writeString(OV_STRING, message);
        WireWriter messageEntry = new WireWriter();
        messageEntry.writeString(1, "message"); // BamlOutboundMapEntry.key = 1
        messageEntry.writeMessage(2, messageValue.toByteArray()); // value = 2

        WireWriter classValue = new WireWriter();
        classValue.writeString(1, classFqn); // BamlValueClass.name = 1
        classValue.writeMessage(2, messageEntry.toByteArray()); // fields = 2

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, classValue.toByteArray());

        WireWriter err = new WireWriter();
        err.writeMessage(1, ov.toByteArray()); // BamlOutboundError.value = 1
        for (String line : traceLines) {
            err.writeString(2, line); // trace = 2 (repeated)
        }

        WireWriter envelope = new WireWriter();
        envelope.writeMessage(2, err.toByteArray()); // BamlOutboundResult.error = 2
        return envelope.toByteArray();
    }

    // -- TypeMismatch → IllegalArgumentException remap -----------------------

    /**
     * An unregistered {@code baml.errors.TypeMismatch} decodes its value to a
     * field {@code Map}, and the remap pulls the exception message from the
     * {@code "message"} entry — NOT a {@link BamlError} wrapper.
     */
    @Test
    @Order(1)
    void type_mismatch_unregistered_remaps_to_iae_with_map_message() {
        byte[] envelope =
                errorEnvelope(TYPE_MISMATCH_FQN, "could not infer a type: T");

        IllegalArgumentException iae =
                assertThrows(
                        IllegalArgumentException.class,
                        () -> ProtoReader.decodeOutboundResult(envelope));
        assertEquals("could not infer a type: T", iae.getMessage());
        // The remap replaces the BamlError wrapper entirely. (`iae instanceof
        // BamlError` no longer even compiles — proof enough — so check
        // reflectively.)
        assertFalse(BamlError.class.isInstance(iae), "TypeMismatch must not surface as BamlError");
    }

    /**
     * When {@code baml.errors.TypeMismatch} is registered, its value decodes to
     * the generated class instance and the remap reads the message via the
     * reflective {@code message()} accessor (the runtime library never references
     * the generated class statically).
     */
    @Test
    @Order(2)
    void type_mismatch_registered_instance_message_via_accessor() {
        TypeRegistry.registerClass(
                TYPE_MISMATCH_FQN, TypeMismatchLike.class.getName(), new String[] {"message"});

        byte[] envelope = errorEnvelope(TYPE_MISMATCH_FQN, "boundary type mismatch");

        IllegalArgumentException iae =
                assertThrows(
                        IllegalArgumentException.class,
                        () -> ProtoReader.decodeOutboundResult(envelope));
        assertEquals("boundary type mismatch", iae.getMessage());
    }

    /** A non-TypeMismatch error value still surfaces as a {@link BamlError}. */
    @Test
    @Order(3)
    void non_type_mismatch_error_stays_baml_error() {
        byte[] envelope =
                errorEnvelope("baml.errors.GenericSdkError", "boom");
        BamlError err =
                assertThrows(
                        BamlError.class, () -> ProtoReader.decodeOutboundResult(envelope));
        assertEquals("baml.errors.GenericSdkError", err.class_name());
    }

    /** The remapped IllegalArgumentException carries the spliced BAML frame. */
    @Test
    @Order(4)
    void type_mismatch_iae_stack_trace_carries_baml_frame() {
        byte[] envelope =
                errorEnvelope(
                        TYPE_MISMATCH_FQN,
                        "mismatch",
                        "File \"resume.baml\", line 12, in user.extract");
        IllegalArgumentException iae =
                assertThrows(
                        IllegalArgumentException.class,
                        () -> ProtoReader.decodeOutboundResult(envelope));

        String rendered = renderStackTrace(iae);
        assertTrue(rendered.contains("resume.baml"), rendered);
        assertTrue(rendered.contains("user.extract"), rendered);
    }

    // -- BamlError-arm splice ------------------------------------------------

    /** A BamlError's printed stack trace renders the spliced BAML source frame. */
    @Test
    @Order(5)
    void baml_error_stack_trace_carries_baml_frame() {
        byte[] envelope =
                errorEnvelope(
                        "baml.errors.GenericSdkError",
                        "boom",
                        "File \"types.baml\", line 3, in user.throws_test.ParseJson");
        BamlError err =
                assertThrows(
                        BamlError.class, () -> ProtoReader.decodeOutboundResult(envelope));

        String rendered = renderStackTrace(err);
        assertTrue(rendered.contains("types.baml"), rendered);
        assertTrue(rendered.contains("user.throws_test.ParseJson"), rendered);
        // The wire trace stays available structurally regardless of the splice.
        assertFalse(err.baml_trace().isEmpty());
    }

    // -- BamlTraceback: trace-line parsing -----------------------------------

    /** A dotted function splits into declaringClass = namespace, methodName = leaf. */
    @Test
    void splice_parses_dotted_frame_into_namespace_and_leaf() {
        RuntimeException exc = new RuntimeException("x");
        BamlTraceback.splice(exc, List.of("File \"a.baml\", line 5, in ns.mid.leaf"));

        StackTraceElement top = exc.getStackTrace()[0];
        assertEquals("ns.mid", top.getClassName());
        assertEquals("leaf", top.getMethodName());
        assertEquals("a.baml", top.getFileName());
        assertEquals(5, top.getLineNumber());
        assertTrue(top.toString().contains("ns.mid.leaf(a.baml:5)"), top.toString());
    }

    /** A namespace-less function keeps the {@code <baml>} declaring-class sentinel. */
    @Test
    void splice_bare_function_uses_baml_sentinel_declaring_class() {
        RuntimeException exc = new RuntimeException("x");
        BamlTraceback.splice(exc, List.of("File \"x.baml\", line 3, in bare"));

        StackTraceElement top = exc.getStackTrace()[0];
        assertEquals("<baml>", top.getClassName());
        assertEquals("bare", top.getMethodName());
        assertEquals(3, top.getLineNumber());
    }

    /** Wire order (most-recent-call-last) is reversed so the throwing frame is on top. */
    @Test
    void splice_reverses_to_most_recent_first() {
        RuntimeException exc = new RuntimeException("x");
        int realDepth = exc.getStackTrace().length;
        BamlTraceback.splice(
                exc,
                List.of(
                        "File \"o.baml\", line 1, in ns.outer",
                        "File \"i.baml\", line 2, in ns.throwing"));

        StackTraceElement[] st = exc.getStackTrace();
        assertEquals("throwing", st[0].getMethodName()); // wire-last → top
        assertEquals("outer", st[1].getMethodName());
        assertEquals(realDepth + 2, st.length); // both baml frames prepended
    }

    /** Malformed lines are individually skipped; parseable ones still splice. */
    @Test
    void splice_skips_malformed_lines() {
        RuntimeException exc = new RuntimeException("x");
        int realDepth = exc.getStackTrace().length;
        BamlTraceback.splice(
                exc,
                List.of(
                        "not a trace line",
                        "File \"g.baml\", line 9, in ns.good",
                        "File \"bad\", line notanumber, in ns.nope"));

        StackTraceElement[] st = exc.getStackTrace();
        assertEquals(realDepth + 1, st.length); // only the one good frame
        assertEquals("good", st[0].getMethodName());
        assertEquals("g.baml", st[0].getFileName());
    }

    /** An empty trace leaves the native stack trace untouched (same array). */
    @Test
    void splice_empty_trace_leaves_stack_untouched() {
        RuntimeException exc = new RuntimeException("x");
        StackTraceElement[] before = exc.getStackTrace();
        BamlTraceback.splice(exc, List.of());
        assertArrayEquals(before, exc.getStackTrace());
    }

    /** A null trace is a no-op and returns the same exception instance. */
    @Test
    void splice_null_trace_is_noop() {
        RuntimeException exc = new RuntimeException("x");
        StackTraceElement[] before = exc.getStackTrace();
        RuntimeException returned = BamlTraceback.splice(exc, null);
        assertSame(exc, returned);
        assertArrayEquals(before, exc.getStackTrace());
    }

    /** A trace of only malformed lines leaves the stack untouched (nothing parsed). */
    @Test
    void splice_all_malformed_leaves_stack_untouched() {
        RuntimeException exc = new RuntimeException("x");
        StackTraceElement[] before = exc.getStackTrace();
        BamlTraceback.splice(exc, List.of("garbage", "also garbage"));
        assertArrayEquals(before, exc.getStackTrace());
    }

    // -- BamlPanic classification --------------------------------------------

    /** BamlPanic is an Error (not an Exception): a bare catch(Exception) misses it. */
    @Test
    void baml_panic_is_error_not_exception() {
        BamlPanic panic = new BamlPanic("boom", List.of(), "baml.panics.SdkPanic");
        assertInstanceOf(Error.class, panic);
        assertInstanceOf(Throwable.class, panic);
        // `panic instanceof Exception` no longer compiles now that BamlPanic
        // extends Error — the compile-time proof of the re-parent — so the
        // negative checks go through the reflective form.
        assertFalse(Exception.class.isInstance(panic), "BamlPanic must not be an Exception");
        assertFalse(
                RuntimeException.class.isInstance(panic),
                "BamlPanic must not be a RuntimeException");
        // Fields + accessors are preserved across the re-parent.
        assertEquals("boom", panic.value());
        assertNotNull(panic.baml_trace());
        assertEquals("baml.panics.SdkPanic", panic.class_name());
    }

    private static String renderStackTrace(Throwable t) {
        StringWriter sw = new StringWriter();
        t.printStackTrace(new PrintWriter(sw));
        return sw.toString();
    }
}
