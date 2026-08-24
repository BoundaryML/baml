// Minimum repro of the generic-method FFI plumbing bug.
//
// `ai.stream.Stream<T, S>.next() -> S | ai.stream.Done` is a generic
// instance method whose return type mentions a class-level TypeVar. The
// host-side lowering for that call doesn't substitute the instantiation's
// `S` into the lifted return type, so the union still contains
// `Ty::TypeVar`, which collapses to `Ty::Void`. The runtime then sees a
// concrete value arrive and fails to find a member of `[Void, ...]` that
// accepts it.
//
// This file isolates the same pattern in a single-shot call, no LLM, no
// streams, no `Done` union — just `WrapperMethods<T>.get_value(self)
// -> T` invoked on a `WrapperMethods<String>` instance. See the Python
// original's module/function docstrings for the full bug narrative.
//
// Port of python_pydantic2/type_shapes/customizable/test_generic.py — same
// test names, same intent.

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.generics.Fns;
import baml_sdk.generics.WrapperMethods;
import org.junit.jupiter.api.Test;

class TestGeneric {

    @Test
    void test_generic_generic() {
        // `WrapperMethods<String>.get_value_or_marker()` should still
        // round-trip a string when the declared return is
        // `T | WrapperMarker`.
        //
        // java-port note (codegen decision, 2026-07-17): the shape of a
        // TypeVar-bearing union (`T | WrapperMarker`) was previously left "TBD"
        // and this port narrowed it with a `Union2.Arm0<?, ?>` wrapper. The
        // explicit-generics slice settles it: a union with a TypeVar arm renders
        // as `java.lang.Object` and decodes to the bare wire value (no arity
        // family — the TypeVar arm inhabits whatever the caller's `T` is), which
        // matches the Python twin's `== "hello"` exactly. So the returned value
        // is just the round-tripped string.
        WrapperMethods<String> w = Fns.make_wrapper_methods("hello");
        assertEquals("hello", w.get_value_or_marker());
    }

    @Test
    void test_generic_generic_wrapper_get_value() {
        // `WrapperMethods<String>.get_value()` should round-trip a string.
        //
        // The engine-side strict path (full-binding Gate A on instance
        // methods) requires the receiver to carry its concrete class type
        // args on the wire. Until outbound decoding preserves the generic
        // parameterization of a returned `WrapperMethods<String>`, the
        // re-encoded receiver has empty class args and the call is
        // rejected at the inbound boundary.
        WrapperMethods<String> w = Fns.make_wrapper_methods("hello");
        assertEquals("hello", w.get_value());
    }
}
