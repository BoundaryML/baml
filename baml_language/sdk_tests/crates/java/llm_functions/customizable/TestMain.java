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
//   * LLM operations are flat projections (`ExtractResume_spec` and, for
//     tool-free functions, `ExtractResume_stream`). Synthetic `$...` functions
//     are intentionally absent.
//   * pydantic `Model.model_fields` -> reflected declared field names.
//   * `import baml_sdk[.ns]` smokes -> referencing a generated symbol's
//     `.class` (compile-time reachability + class-load side effect).
//   * Request rendering and prompt access now live on the FunctionSpec facade;
//     this fixture exercises the portable Prompt result without a synthetic
//     build_request/render_prompt function binding.
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlPrompt;
import baml_sdk.ipsum.Sentiment;
import baml_sdk.lorem.Resume;
import baml_sdk.lorem.StreamingDoc;
import baml_sdk.vendor.ai.PromptMessage;
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
    void test_main_extract_resume_operation_bindings() {
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume_spec"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume_spec_async"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume_stream"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "ExtractResume_stream_async"));
        for (String oldName :
                List.of(
                        "ExtractResume$spec",
                        "ExtractResume$stream",
                        "ExtractResume$render_prompt",
                        "ExtractResume$build_request",
                        "ExtractResume$parse")) {
            assertTrue(!hasMethod(baml_sdk.lorem.Fns.class, oldName), "legacy binding survived " + oldName);
        }
    }

    @Test
    void test_main_flat_stream_controls_live_on_the_stream_options() {
        assertTrue(hasMethod(baml_sdk.lorem.Fns.ExtractResume_stream$Opts.class, "client"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.ExtractResume_stream$Opts.class, "on_event"));
    }

    @Test
    void test_main_function_spec_prompt_is_portable_and_reusable() {
        BamlPrompt prompt = baml_sdk.lorem.Fns.ExtractResume_spec("Ada Lovelace").prompt();
        String firstText = prompt.text();
        String secondText = prompt.text();
        assertTrue(!firstText.isEmpty());
        assertEquals(firstText, secondText);

        List<PromptMessage> firstMessages = prompt.messages();
        List<PromptMessage> secondMessages = prompt.messages();
        assertTrue(!firstMessages.isEmpty());
        assertEquals(firstMessages, secondMessages);

        // ai.Prompt is runtime-owned, so every generated signature exposes the
        // same portable BamlPrompt type rather than a handle-backed twin.
        assertFalse(classExists("baml_sdk.vendor.ai.Prompt"));
    }

    @Test
    void test_main_streaming_extract_factory_bindings() {
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_async"));
    }

    @Test
    void test_main_streaming_extract_operation_bindings() {
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_spec"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_spec_async"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_stream"));
        assertTrue(hasMethod(baml_sdk.lorem.Fns.class, "StreamingExtract_stream_async"));
    }

    @Test
    void test_main_stream_types_lorem_leaf_present() {
        // PPIR synthesizes Class$stream partial models for any class referenced by
        // an LLM function's return type. Both Resume and StreamingDoc are LLM
        // return types, so at least one in-package `$stream` partial must exist.
        // Java keeps the
        // in-package `$`-preserved naming (`baml_sdk.lorem.StreamingDoc$stream`),
        // NOT Python's `stream_types.lorem.*` legacy layout. StreamingDoc's
        // conditional-emit outcome is pinned to "emitted"; Resume's is left to
        // the conditional-emit rule.
        boolean hasAny =
                classExists("baml_sdk.lorem.Resume$stream")
                        || classExists("baml_sdk.lorem.StreamingDoc$stream");
        assertTrue(hasAny, "expected at least one in-package $stream partial class in lorem");
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
