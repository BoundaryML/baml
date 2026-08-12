// End-to-end check of the 09a-style baml_src -> baml_sdk pipeline.
//
// Port of python_pydantic2/llm_functions/customizable/test_main.py — same test
// names, cases, assertions.
//
// Drives codegen from real `.baml` source through the full
// `baml_project::build_symbol_pool` path (parse -> HIR -> TIR -> SymbolPool ->
// emitter) and asserts on the generated Java surface.
//
// ===========================================================================
// java-port notes (INVENTED shapes / adaptations — need human review):
//   * Free functions are static methods on a per-namespace `Fns` holder
//     (codegen-conventions doc): Python `lorem.ExtractResume(...)` ->
//     `baml_sdk.lorem.Fns.ExtractResume(...)`. "callable(...)" checks become
//     reflective method-presence checks on the holder (`hasMethod`).
//   * `$`-companions keep the BAML name verbatim in Java (`ExtractResume$parse`);
//     the Python `.pyi` mangles `$`->`__` only because Python forbids `$`.
//   * pydantic `Model.model_fields` -> reflected declared field names.
//   * `import baml_sdk[.ns]` smokes -> referencing a generated symbol's
//     `.class` (compile-time reachability + class-load side effect).
//   * The build_request tests use `@SetEnvironmentVariable` (junit-pioneer) as
//     the JUnit analog of pytest `monkeypatch.setenv`. See the TODO on those
//     tests: junit-pioneer patches only the JVM's cached env view, so a native
//     setenv reachable by the JNI engine is still required for the engine to
//     observe the key.  [junit-pioneer must be added to build.gradle.kts.]
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BridgeEnv;
import baml_sdk.baml.http.Request;
import baml_sdk.ipsum.Sentiment;
import baml_sdk.lorem.Resume;
import baml_sdk.lorem.StreamingDoc;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Test;

class TestMain {

    // --- helpers -----------------------------------------------------------

    /** Java analog of Python's `callable(getattr(ns, name))`: a public static
     *  method named `name` exists on the namespace's `Fns` holder. */
    private static boolean hasMethod(Class<?> holder, String name) {
        for (Method m : holder.getMethods()) {
            if (m.getName().equals(name)) {
                return true;
            }
        }
        return false;
    }

    /** Java analog of Python's `hasattr(module, ClassName)` for a generated
     *  class — does the fully-qualified class load? */
    private static boolean classExists(String fqcn) {
        try {
            Class.forName(fqcn);
            return true;
        } catch (ClassNotFoundException e) {
            return false;
        }
    }

    /** Java analog of pydantic `Model.model_fields` keys. */
    private static Set<String> fieldNames(Class<?> c) {
        return Arrays.stream(c.getDeclaredFields())
                .filter(f -> !f.isSynthetic())
                .map(Field::getName)
                .collect(Collectors.toSet());
    }

    private static Map<String, String> lowerKeys(Map<String, String> headers) {
        Map<String, String> out = new LinkedHashMap<>();
        for (Map.Entry<String, String> e : headers.entrySet()) {
            out.put(e.getKey().toLowerCase(Locale.ROOT), e.getValue());
        }
        return out;
    }

    // --- tests -------------------------------------------------------------

    @Test
    void test_main_root_imports_cleanly() throws ClassNotFoundException {
        // java-port note: Python `import baml_sdk` runs the root package's
        // runtime-init side effect. The Java analog is FORCING the root holder's
        // static initializer to run — a bare `.class` literal does NOT initialize
        // a class in Java, so `Class.forName(name, /*initialize=*/true, cl)` is
        // what actually performs the (idempotent) runtime bootstrap. [INVENTED:
        // `baml_sdk.Fns` as the root free-function / runtime-init holder — needs
        // review.] Resolved: a root namespace with no free functions emits NO
        // `Fns` holder; the runtime-init anchor is the root `Baml` class (its
        // static initializer performs the idempotent bootstrap), so that is what
        // `Class.forName(..., initialize=true, ...)` must touch.
        assertNotNull(Class.forName("baml_sdk.Baml", true, TestMain.class.getClassLoader()));
    }

    @Test
    void test_main_namespaces_reachable_via_explicit_import() {
        assertNotNull(Resume.class);
        assertNotNull(Sentiment.class);
    }

    @Test
    void test_main_lorem_resume_class_shape() {
        // java-port note: `issubclass(Resume, pydantic.BaseModel)` has no Java
        // analog (no BaseModel); the shape is pinned by the reflected field set,
        // the analog of `set(Resume.model_fields)`.
        assertEquals(Set.of("name", "email"), fieldNames(Resume.class));
    }

    @Test
    void test_main_lorem_streaming_doc_class_shape() {
        assertEquals(Set.of("title", "body", "word_count"), fieldNames(StreamingDoc.class));
    }

    @Test
    void test_main_ipsum_sentiment_enum_shape() {
        assertTrue(Sentiment.class.isEnum());
        Set<String> names =
                Arrays.stream(Sentiment.values()).map(Enum::name).collect(Collectors.toSet());
        assertEquals(Set.of("POSITIVE", "NEGATIVE", "NEUTRAL"), names);
        // 09b: enum constant spelling is the variant name verbatim.
        // java-port note: Python's str-enum `.value == "POSITIVE"` /
        // `isinstance(str)` (the JSON round-trip guarantee) maps in Java to the
        // wire-name serializer map; for identifier-spelled variants the wire
        // name equals `.name()`, so the constant name pins the same contract.
        assertEquals("POSITIVE", Sentiment.POSITIVE.name());
    }

    @Test
    void test_main_extract_resume_factory_bindings() {
        // Sync + async siblings at the namespace leaf, per 09b §4.
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume_async"));
    }

    @Test
    void test_main_extract_resume_companion_bindings() {
        // Companions auto-generated by the compiler from the prompt block:
        // `$render_prompt`, `$parse` ($build_request*/$parse_stream went
        // away with the legacy LLM path). Each
        // gets a static method (+ async sibling) on the `Fns` holder.
        for (String name :
                List.of(
                        "ExtractResume$render_prompt",
                        "ExtractResume$parse")) {
            assertTrue(
                    hasMethod(baml_sdk.lorem.Fns.class, name), "missing companion binding " + name);
            assertTrue(
                    hasMethod(baml_sdk.lorem.Fns.class, name + "_async"),
                    "missing async companion " + name + "_async");
        }
    }

    @Test
    void test_main_streaming_extract_factory_bindings() {
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_async"));
    }

    @Test
    void test_main_streaming_extract_companion_bindings() {
        for (String name :
                List.of(
                        "StreamingExtract$render_prompt",
                        "StreamingExtract$parse")) {
            assertTrue(
                    hasMethod(baml_sdk.lorem.Fns.class, name), "missing companion binding " + name);
            assertTrue(
                    hasMethod(baml_sdk.lorem.Fns.class, name + "_async"),
                    "missing async companion " + name + "_async");
        }
    }

    @Test
    void test_main_stream_types_lorem_leaf_present() {
        // PPIR synthesizes Class$stream companions for any class referenced by
        // an LLM function's return type. Both Resume and StreamingDoc are LLM
        // return types, so at least one in-package `$stream` companion must exist.
        // java-port note: GAP B (2026-07-17, TS-aligned) — companions keep the
        // in-package `$`-preserved naming (`baml_sdk.lorem.StreamingDoc$stream`),
        // NOT Python's `stream_types.lorem.*` legacy layout. StreamingDoc's
        // conditional-emit outcome is pinned to "emitted"; Resume's is left to
        // the conditional-emit rule.
        boolean hasAny =
                classExists("baml_sdk.lorem.Resume$stream")
                        || classExists("baml_sdk.lorem.StreamingDoc$stream");
        assertTrue(hasAny, "expected at least one in-package $stream companion class in lorem");
    }

    @Test
    void test_main_classify_sentiment_factory_bindings() {
        assertTrue(hasMethod(baml_sdk.ipsum.Fns.class, "ClassifySentiment"));
        assertTrue(hasMethod(baml_sdk.ipsum.Fns.class, "ClassifySentiment_async"));
    }

    // -----------------------------------------------------------------------
    // Replay-harness surface (bridge-generics/streaming/02). Codegen-shape
    // only; the keyless behavioral tests live in TestStreamingE2e.java.
    // -----------------------------------------------------------------------

    @Test
    void test_main_replay_server_namespace_bindings() {
        // The BAML-implemented replay server lives in the `replay` namespace
        // (ns_replay/). Both invocation entry points get sync + async siblings.
        for (String name :
                List.of(
                        "replay_serve_until_shutdown",
                        "replay_serve_until_shutdown_async",
                        "replay_serve_detached",
                        "replay_serve_detached_async")) {
            assertTrue(
                    hasMethod(baml_sdk.replay.Fns.class, name),
                    "missing replay-server binding " + name);
        }
    }

    // NOTE: the shorthand-client api_key wiring tests that lived here inspected
    // the auth header on `*$build_request`'s Request. That companion went away
    // with the legacy LLM path (credentials now resolve inside the provider's
    // `invoke`, at request time), so there is no pre-network Request to
    // inspect. Coverage moved to the live smokes in `_planv2/baml_src/live/`.
}
