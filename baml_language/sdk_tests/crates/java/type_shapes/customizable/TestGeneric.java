// Minimum repro of the generic-method FFI plumbing bug.
//
// `Stream<T, S>.next() -> S | baml.stream.StreamFinished` is a generic
// instance method whose return type mentions a class-level TypeVar. The
// host-side lowering for that call doesn't substitute the instantiation's
// `S` into the lifted return type, so the union still contains
// `Ty::TypeVar`, which collapses to `Ty::Void`. The runtime then sees a
// concrete value arrive and fails to find a member of `[Void, ...]` that
// accepts it.
//
// This file isolates the same pattern in a single-shot call, no LLM, no
// streams, no `StreamFinished` union — just `WrapperMethods<T>.get_value(self)
// -> T` invoked on a `WrapperMethods<String>` instance. See the Python
// original's module/function docstrings for the full bug narrative.
//
// Port of python_pydantic2/type_shapes/customizable/test_generic.py — same
// test names, same intent.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.Union2;
import baml_sdk.generics.Fns;
import baml_sdk.generics.WrapperMethods;
import org.junit.jupiter.api.Test;

class TestGeneric {

    @Test
    void test_generic() {
        // `WrapperMethods<String>.get_value_or_marker()` should still
        // round-trip a string when the declared return is
        // `T | WrapperMarker`.
        //
        // java-port note: `get_value_or_marker` returns `T | WrapperMarker`
        // — a union whose first arm is the *class's* TypeVar rather than a
        // concrete BAML type, so it renders as `Union2<Object, WrapperMarker>`
        // (Arm0 = the `T` arm, Arm1 = `WrapperMarker`); the descriptor keeps
        // the arm as `tv:T` and the Java type arg erases to `Object`. The
        // conventions doc leaves the shape of a TypeVar-bearing union return
        // type as an open question (the "Generic function/method (explicit)"
        // row in ref-java-state-of-completeness.md: "shape TBD"). This port
        // narrows the returned value with a wildcard `Union2.Arm0<?, ?>`
        // pattern and reads `.value()` (Java 17: `instanceof` only, no record
        // patterns). This needs a real codegen decision — flagged for review.
        WrapperMethods<String> w = Fns.make_wrapper_methods("hello");
        Object result = w.get_value_or_marker();
        assertTrue(
                result instanceof Union2.Arm0<?, ?> tv
                        && "hello".equals(tv.value()));
    }

    @Test
    void test_generic_wrapper_get_value() {
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
