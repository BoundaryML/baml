// Static + instance method coverage (ns_methods_on_classes.Greeter).
//
// `raises_test.DocLoader` pins the *shape* of method bindings, but its bodies
// always throw, so they are never invoked. `Greeter` has non-throwing bodies,
// so this exercises the full host->engine round-trip for both flavors:
//   - static   -> `Greeter.create(name)`          (no receiver)
//   - instance -> `g.who()` / `g.greet(arg)`      (receiver is `self`)
// each with its `_async` sibling returning `CompletableFuture<T>`.
//
// Port of python_pydantic2/function_calls/customizable/test_methods_on_classes.py.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import baml_sdk.methods_on_classes.Greeter;
import java.util.concurrent.CompletableFuture;
import java.util.function.BiFunction;
import java.util.function.Function;
import org.junit.jupiter.api.Test;

class TestMethodsOnClasses {

    @Test
    void test_methods_on_classes_method_bindings_exist() {
        // java-port note: Python asserts `callable(Greeter.create)` etc. on the
        // (un)bound attributes. The Java analog is that each binding resolves as
        // a method reference at compile time (static bindings hang off the class;
        // instance bindings take the receiver as the unbound first parameter).
        Function<String, Greeter> create = Greeter::create;
        Function<String, CompletableFuture<Greeter>> createAsync = Greeter::create_async;
        Function<Greeter, String> who = Greeter::who;
        Function<Greeter, CompletableFuture<String>> whoAsync = Greeter::who_async;
        BiFunction<Greeter, String, String> greet = Greeter::greet;
        BiFunction<Greeter, String, CompletableFuture<String>> greetAsync = Greeter::greet_async;

        assertNotNull(create);
        assertNotNull(createAsync);
        assertNotNull(who);
        assertNotNull(whoAsync);
        assertNotNull(greet);
        assertNotNull(greetAsync);
    }

    @Test
    void test_methods_on_classes_static_create_round_trips() {
        Greeter g = Greeter.create("ada");
        assertInstanceOf(Greeter.class, g);
        assertEquals("ada", g.name());
    }

    @Test
    void test_methods_on_classes_static_create_async_round_trips() {
        Greeter g = Greeter.create_async("grace").join();
        assertInstanceOf(Greeter.class, g);
        assertEquals("grace", g.name());
    }

    @Test
    void test_methods_on_classes_instance_who_round_trips() {
        Greeter g = Greeter.create("hopper");
        assertEquals("hopper", g.who());
    }

    @Test
    void test_methods_on_classes_instance_who_async_round_trips() {
        Greeter g = Greeter.create_async("hopper").join();
        assertEquals("hopper", g.who_async().join());
    }

    @Test
    void test_methods_on_classes_instance_greet_with_arg_round_trips() {
        Greeter g = Greeter.create("lovelace");
        assertEquals("hi", g.greet("hi"));
    }

    @Test
    void test_methods_on_classes_instance_greet_async_with_arg_round_trips() {
        Greeter g = Greeter.create_async("lovelace").join();
        assertEquals("hi", g.greet_async("hi").join());
    }
}
