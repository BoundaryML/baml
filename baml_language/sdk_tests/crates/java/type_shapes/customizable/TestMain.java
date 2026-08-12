// Smoke tests for the type_shapes sdk-test crate.
//
// Port of python_pydantic2/type_shapes/customizable/test_main.py — same
// test names, same intent.
//
// java-port note: Python's smoke tests are `import baml_sdk.<ns>` module
// imports — a namespace "imports cleanly" if the import statement doesn't
// raise. Per the conventions doc's "Namespace-import smoke tests" rule,
// the Java analog is referencing a known generated symbol's `.class`
// (compile-time reachability, plus the class-load side effect that
// triggers runtime init per the doc's "Runtime init" row). Each namespace
// below is represented by one real symbol declared in that namespace's
// fixture BAML source.
import static org.junit.jupiter.api.Assertions.assertNotNull;

import baml_sdk.Foo;
import baml_sdk.a.b.Thing;
import baml_sdk.lorem.Resume;
import org.junit.jupiter.api.Test;

class TestMain {

    @Test
    void test_main_root_imports_cleanly() {
        assertNotNull(baml_sdk.Fns.class);
    }

    @Test
    void test_main_all_namespaces_reachable() {
        assertNotNull(baml_sdk.primitives.Primitives.class);
        assertNotNull(baml_sdk.media.Media.class);
        assertNotNull(baml_sdk.enums.Sentiment.class);
        assertNotNull(baml_sdk.literals.Literals.class);
        assertNotNull(baml_sdk.class_refs.Outer.class);
        assertNotNull(baml_sdk.aliases.AliasContainer.class);
        assertNotNull(baml_sdk.aliases_consumer.RecListConsumer.class);
        assertNotNull(baml_sdk.optional.OptionalContainer.class);
        assertNotNull(baml_sdk.lists.ListContainer.class);
        assertNotNull(baml_sdk.maps.MapContainer.class);
        assertNotNull(baml_sdk.unions.UnionContainer.class);
        assertNotNull(baml_sdk.recursion.IntBinaryTree.class);
        assertNotNull(baml_sdk.generics.Wrapper.class);
        assertNotNull(baml_sdk.forward_refs.Other.class);
        assertNotNull(baml_sdk.complex_models.ComplexProfile.class);
        assertNotNull(baml_sdk.lorem.Resume.class);
        assertNotNull(baml_sdk.a.Fns.class);
    }

    @Test
    void test_main_root_foo_reachable() {
        assertNotNull(Foo.class);
    }

    @Test
    void test_main_lorem_resume_reachable() {
        assertNotNull(Resume.class);
    }

    @Test
    void test_main_deep_namespace_thing_reachable() {
        assertNotNull(Thing.class);
    }
}
