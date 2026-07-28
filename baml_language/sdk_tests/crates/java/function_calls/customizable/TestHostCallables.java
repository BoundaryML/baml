// End-to-end tests for the host-callable round trip.
//
// The BAML fixture declares functions whose first parameter is a typed
// `Callable`. The Java test passes a `java.util.function.*` value (or a
// generated functional interface where the arity/optionals don't fit
// Function/BiFunction); the bridge auto-registers it in the host-value
// registry, the engine binds it to an `Object::HostClosure`, and BAML invokes
// it back through the dispatch FFI.
//
// Port of python_pydantic2/function_calls/customizable/test_host_callables.py.
//
// Host-callable arity mapping (ref-java-state-of-completeness.md):
//   (int) -> string        -> java.util.function.Function<Long, String>
//   (int, string) -> string-> java.util.function.BiFunction<Long, String, String>
//   (int) -> int           -> java.util.function.Function<Long, Long>
//   (Person) -> string     -> java.util.function.Function<Person, String>
//   (x:int, y?:int, z?:int) -> int  -> a GENERATED functional interface
//       (see IntOptCallback / IntOpts below — INVENTED, flagged in the report:
//       Function/BiFunction cannot express a trailing optional-args `$opts`
//       bag, which the bridge folds the supplied optionals into, mirroring the
//       TS `$opts` reshape).
//
// java-port note (exception identity): where Python raises inside a callback
// and asserts the ORIGINAL exception round-trips (`raised is caught`), the Java
// port throws an unchecked exception instance and asserts `assertSame`.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;

import baml_bridge.BamlError;
import baml_sdk.host_callable_tests.Fns;
import baml_sdk.host_callable_tests.IntOptCallback;
import baml_sdk.host_callable_tests.Person;
import baml_sdk.host_callable_tests.ValidationError;
import java.util.ArrayList;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.function.BiFunction;
import java.util.function.Function;
import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Timeout;

class TestHostCallables {

    // A user-defined unchecked exception, the analog of Python's custom
    // `MyDomainError(Exception)` with an extra field.
    static final class MyDomainError extends RuntimeException {
        final long code;

        MyDomainError(String message, long code) {
            super(message);
            this.code = code;
        }
    }

    @Test
    void test_host_callables_simple_sync_callable_returns_string() {
        Function<Long, String> cb = x -> "got " + x;

        String result = Fns.call_with_callback(cb, 5L);
        assertEquals("got 5", result);
    }

    @Test
    void test_host_callables_two_arg_callable_unpacks_positional_args() {
        BiFunction<Long, String, String> cb = (x, prefix) -> prefix + ":" + x;

        String result = Fns.call_with_two_args(cb, 7L, "answer");
        assertEquals("answer:7", result);
    }

    @Test
    void test_host_callables_int_return_callable_round_trip() {
        Function<Long, Long> cb = x -> x * 2;

        long result = Fns.call_int_callback(cb, 21L);
        assertEquals(42L, result);
    }

    @Test
    void test_baml_closure_is_a_native_callable_with_host_language_arguments() {
        Function<Long, Long> addTen = Fns.make_adder(10L);
        assertEquals(15L, addTen.apply(5L));
        assertEquals(17L, addTen.apply(7L));
    }

    @Test
    void test_baml_closure_decodes_multiple_args_and_structured_return_values() {
        BiFunction<Long, String, Person> build = Fns.make_pair_builder(30L);
        assertEquals(new Person("Ada", 42L), build.apply(12L, "Ada"));
        assertEquals(new Person("Grace", 35L), build.apply(5L, "Grace"));
    }

    @Test
    void test_baml_closure_is_reusable_and_retains_mutable_captures() {
        java.util.function.Supplier<Long> nextValue = Fns.make_counter(40L);
        assertEquals(41L, nextValue.get());
        assertEquals(42L, nextValue.get());
    }

    @Test
    void test_host_callables_throwing_callable_round_trips_original_python_exception() {
        // A native Java exception raised inside a host callable surfaces back to
        // the caller as the *same* exception object (identity), not flattened
        // into a BamlError(HostCallable(...)) wrapper.
        RuntimeException raised = new RuntimeException("nope");

        Function<Long, String> cb = x -> {
            throw raised;
        };

        RuntimeException caught =
                assertThrows(RuntimeException.class, () -> Fns.call_with_callback(cb, 1L));
        assertSame(raised, caught);
        assertEquals("nope", caught.getMessage());
    }

    @Test
    void test_host_callables_throwing_callable_keyerror_round_trips_with_identity() {
        // The rehydration path is class-agnostic: any exception round-trips by
        // reference. (Python's KeyError -> Java's NoSuchElementException, the
        // nearest "missing key" analog.)
        NoSuchElementException raised = new NoSuchElementException("missing");

        Function<Long, String> cb = x -> {
            throw raised;
        };

        NoSuchElementException caught =
                assertThrows(NoSuchElementException.class, () -> Fns.call_with_callback(cb, 1L));
        assertSame(raised, caught);
    }

    @Test
    void test_host_callables_throwing_callable_custom_python_exception_round_trips_with_identity() {
        // A user-defined exception subclass also round-trips by identity — the
        // bridge doesn't care about the concrete type.
        MyDomainError raised = new MyDomainError("custom domain failure", 42L);

        Function<Long, String> cb = x -> {
            throw raised;
        };

        MyDomainError caught =
                assertThrows(MyDomainError.class, () -> Fns.call_with_callback(cb, 1L));
        assertSame(raised, caught);
        assertEquals(42L, caught.code);
    }

    @Test
    void test_host_callables_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml() {
        // When the host throws BamlError(value=<codegenned BAML class>), the
        // bridge unwraps the inner value and emits it as that real BAML class on
        // the wire, so BAML's typed `catch (e: ValidationError)` matches
        // structurally and reads real fields.
        Function<Long, String> cb = x -> {
            throw new BamlError(
                    new ValidationError(4L, "bad shape", List.of("name", "age", "email", "phone")));
        };

        String result = Fns.call_with_typed_throws(cb, 1L);
        assertEquals("caught: bad shape", result);
    }

    @Test
    void test_host_callables_throwing_callable_bamlerror_propagates_back_with_typed_fields() {
        // The same BamlError(ValidationError(...)), when NOT caught in BAML,
        // propagates back out with all typed fields preserved.
        BamlError raised =
                new BamlError(new ValidationError(7L, "propagated through", List.of("x", "y")));

        Function<Long, String> cb = x -> {
            throw raised;
        };

        BamlError caught =
                assertThrows(
                        BamlError.class, () -> Fns.call_with_typed_throws_propagating(cb, 1L));
        ValidationError decoded = assertInstanceOf(ValidationError.class, caught.value());
        assertEquals(7L, decoded.code());
        assertEquals("propagated through", decoded.message());
        assertEquals(List.of("x", "y"), decoded.fields());
    }

    @Test
    void test_host_callables_throwing_async_callable_round_trips_original_python_exception() {
        // java-port note: Python's callback is an `async def` that raises; the
        // dispatch path runs the coroutine. Java has no coroutine detection, and
        // an async host callable would be a distinct
        // `Function<Long, CompletableFuture<String>>` overload. The identity
        // contract is what this pins, so we throw synchronously and assert the
        // same object surfaces (equivalent observable behavior).
        RuntimeException raised = new RuntimeException("async nope");

        Function<Long, String> cb = x -> {
            throw raised;
        };

        RuntimeException caught =
                assertThrows(RuntimeException.class, () -> Fns.call_with_callback(cb, 1L));
        assertSame(raised, caught);
    }

    @Test
    void test_host_callables_multiple_throws_in_flight_do_not_collide_in_registry() {
        // Each host throw mints a fresh host-value key; calls in quick succession
        // must not see the wrong original exception.
        RuntimeException raisedFirst = new RuntimeException("first");
        RuntimeException raisedSecond = new RuntimeException("second");

        Function<Long, String> cbFirst = x -> {
            throw raisedFirst;
        };
        Function<Long, String> cbSecond = x -> {
            throw raisedSecond;
        };

        RuntimeException ei1 =
                assertThrows(RuntimeException.class, () -> Fns.call_with_callback(cbFirst, 1L));
        RuntimeException ei2 =
                assertThrows(RuntimeException.class, () -> Fns.call_with_callback(cbSecond, 2L));
        assertSame(raisedFirst, ei1);
        assertSame(raisedSecond, ei2);
        assertNotSame(ei1, ei2);
    }

    @Test
    @Timeout(value = 60, unit = TimeUnit.SECONDS)
    void test_host_callables_concurrent_throws_in_flight_rehydrate_to_their_own_object() throws Exception {
        // The sequential test above proves keys don't collide back-to-back; this
        // proves the host-value registry (BamlFfi.HOST_VALUES) stays isolated
        // under genuine PARALLELISM — two callables throwing distinct exceptions
        // on concurrent engine calls must each rehydrate to ITS original object
        // by identity, never crossing keys.
        RuntimeException raisedFirst = new RuntimeException("first-concurrent");
        RuntimeException raisedSecond = new RuntimeException("second-concurrent");

        Function<Long, String> cbFirst = x -> {
            throw raisedFirst;
        };
        Function<Long, String> cbSecond = x -> {
            throw raisedSecond;
        };

        int rounds = 32;
        ExecutorService pool = Executors.newFixedThreadPool(2);
        try {
            List<Future<?>> futures = new ArrayList<>();
            for (int i = 0; i < rounds; i++) {
                futures.add(
                        pool.submit(
                                () -> {
                                    RuntimeException caught =
                                            assertThrows(
                                                    RuntimeException.class,
                                                    () -> Fns.call_with_callback(cbFirst, 1L));
                                    assertSame(raisedFirst, caught);
                                }));
                futures.add(
                        pool.submit(
                                () -> {
                                    RuntimeException caught =
                                            assertThrows(
                                                    RuntimeException.class,
                                                    () -> Fns.call_with_callback(cbSecond, 2L));
                                    assertSame(raisedSecond, caught);
                                }));
            }
            // Propagate any assertion failure raised on a worker thread.
            for (Future<?> f : futures) {
                f.get();
            }
        } finally {
            pool.shutdownNow();
        }
    }

    @Disabled(
            "java-port note: mirrors Python's xfail (strict=False). Host-callable "
                    + "release fires only when the engine GCs the Object::HostClosure on "
                    + "its heap; one BAML call rarely triggers the GC heuristic. Java's "
                    + "WeakReference + System.gc() is non-deterministic too, so this is "
                    + "skipped (TS uses it.skip for the same reason).")
    @Test
    void test_host_callables_release_fires_on_drop_of_callable() {
        // Intentionally empty — see @Disabled reason.
    }

    @Test
    void test_host_callables_lambda_round_trip() {
        // Lambdas hit the callable-encoding branch just like named functions.
        String result = Fns.call_with_callback(x -> "lambda-" + x, 99L);
        assertEquals("lambda-99", result);
    }

    @Test
    void test_host_callables_async_callable_runs_to_completion() {
        // java-port note: Python detects a coroutine return and runs it to
        // completion on the dispatch thread. Java's `Function<Long,String>` slot
        // is synchronous; the async-callable surface would be a distinct
        // CompletableFuture-returning overload. To preserve the "runs to
        // completion" contract we drive a CompletableFuture inside the callback
        // and return its resolved value.
        String result =
                Fns.call_with_callback(
                        x -> CompletableFuture.supplyAsync(() -> "async-" + x).join(), 4L);
        assertEquals("async-4", result);
    }

    @Test
    void test_host_callables_async_callable_returning_future_is_awaited_by_bridge() {
        // The FAITHFUL Java spelling of Python's `async def` host callable: the
        // callback RETURNS a CompletableFuture<String> rather than a String, and
        // the bridge (BamlFfi.runHostDispatch, design point C — value-level async
        // detection) awaits it via `whenComplete` before encoding the resolved
        // value back to BAML. This is the counterpart of Python's dispatcher
        // running a returned coroutine to completion.
        //
        // The generated slot types the return as the VALUE (`Function<Long,
        // String>`), so returning a future needs the raw/unchecked idiom:
        // build the callable as `Function<Long, CompletableFuture<String>>`, then
        // launder it to the declared `Function<Long, String>` through a wildcard
        // cast (documented @SuppressWarnings). At runtime `Function` is `Function`
        // — the erased `apply` returns the CompletableFuture the dispatcher then
        // awaits — so there is no ClassCastException.
        Function<Long, CompletableFuture<String>> asyncCb =
                x -> CompletableFuture.supplyAsync(() -> "async-future-" + x);
        @SuppressWarnings("unchecked")
        Function<Long, String> cb = (Function<Long, String>) (Function<Long, ?>) asyncCb;

        String result = Fns.call_with_callback(cb, 7L);
        assertEquals("async-future-7", result);
    }

    @Test
    void test_host_callables_async_callable_future_completing_exceptionally_round_trips_original() {
        // Failure half of the value-level async path: when the returned future
        // completes EXCEPTIONALLY, the bridge's `whenComplete` error branch drives
        // the same design-point-D exception path as a synchronous throw — the
        // ORIGINAL Throwable is registered as a host-opaque value and rehydrated
        // back to the caller by identity (`assertSame`), never flattened. Same
        // raw/unchecked laundering as the success case above.
        RuntimeException raised = new RuntimeException("async-future-boom");
        Function<Long, CompletableFuture<String>> asyncCb =
                x -> CompletableFuture.failedFuture(raised);
        @SuppressWarnings("unchecked")
        Function<Long, String> cb = (Function<Long, String>) (Function<Long, ?>) asyncCb;

        RuntimeException caught =
                assertThrows(RuntimeException.class, () -> Fns.call_with_callback(cb, 1L));
        assertSame(raised, caught);
        assertEquals("async-future-boom", caught.getMessage());
    }

    @Test
    void test_host_callables_multiple_callable_keys_are_distinct() {
        long[] counterA = {0};
        long[] counterB = {0};

        Function<Long, String> cbA = x -> {
            counterA[0] += 1;
            return "a:" + x;
        };
        Function<Long, String> cbB = x -> {
            counterB[0] += 1;
            return "b:" + x;
        };

        assertEquals("a:1", Fns.call_with_callback(cbA, 1L));
        assertEquals("b:2", Fns.call_with_callback(cbB, 2L));
        assertEquals(1L, counterA[0]);
        assertEquals(1L, counterB[0]);
    }

    @Test
    void test_host_callables_class_callback_round_trips_pydantic_model() {
        // A user-defined `Person` class round-trips through the callable
        // boundary: the engine encodes it, the dispatcher decodes it into the
        // generated value class, and the callback receives a `Person`.
        Function<Person, String> cb = p -> p.name() + " is " + p.age();

        Person person = new Person("Ada", 37L);
        String result = Fns.call_with_class_callback(cb, person);
        assertEquals("Ada is 37", result);
    }

    @Test
    void test_host_callables_call_repeatedly_invokes_callback_n_times() {
        List<Long> invocations = new ArrayList<>();
        Function<Long, String> cb = x -> {
            invocations.add(x);
            return "item-" + x;
        };

        List<String> results = Fns.call_repeatedly(cb, 5L);
        assertEquals(List.of("item-0", "item-1", "item-2", "item-3", "item-4"), results);
        assertEquals(List.of(0L, 1L, 2L, 3L, 4L), invocations);
    }

    @Test
    void test_host_callables_call_repeatedly_with_zero_n_returns_empty_list() {
        List<Long> invocations = new ArrayList<>();
        Function<Long, String> cb = x -> {
            invocations.add(x);
            return "";
        };

        List<String> results = Fns.call_repeatedly(cb, 0L);
        assertEquals(List.of(), results);
        assertEquals(List.of(), invocations);
    }

    @Test
    void test_host_callables_call_with_throwing_in_baml_catches_host_callable_error() {
        // The BAML `catch (e)` arm intercepts a host-thrown
        // `baml.errors.HostCallable` and returns `"caught:" + e.class_name`.
        //
        // java-port note: Python's RuntimeError yields class_name "RuntimeError";
        // a Java RuntimeException yields class_name "RuntimeException", so the
        // recovery string differs from the Python original accordingly.
        Function<Long, String> cb = x -> {
            throw new RuntimeException("boom from host");
        };

        String result = Fns.call_with_throwing(cb, 1L);
        assertEquals("caught:RuntimeException", result);
    }

    // ---------------------------------------------------------------------
    // Optional args x host callables (the combination). The callback's own
    // type carries optional parameters `(x:int, y?:int, z?:int) -> int`.
    // Defaults aren't allowed inside a callable type, so the host's own
    // fallback (`!= null ? .. : default`) is the only source of a value when
    // BAML omits an optional. Returns `x*100 + y*10 + z` so each test reads off
    // which optionals were delivered.
    // ---------------------------------------------------------------------

    private static final IntOptCallback optional_args_cb =
            (x, $opts) ->
                    x * 100
                            + ($opts.y() != null ? $opts.y() : 8) * 10
                            + ($opts.z() != null ? $opts.z() : 9);

    @Test
    void test_host_callables_optional_args_all_unset_apply_host_defaults() {
        // `callback(x)` supplies neither optional -> both dropped -> host
        // defaults 8/9 -> 5*100 + 8*10 + 9 = 589.
        assertEquals(
                List.of(589L),
                Fns.call_callback_with_optional_args_all_unset(optional_args_cb, 5L));
    }

    @Test
    void test_host_callables_optional_args_partially_set_deliver_by_name() {
        // `callback(x, y=2)` (500 + 20 + 9 = 529) then `callback(x, z=3)`
        // (500 + 80 + 3 = 583). Optionals cross by name; the omitted one falls
        // back to the host default.
        assertEquals(
                List.of(529L, 583L),
                Fns.call_callback_with_optional_args_partially_set(optional_args_cb, 5L));
    }

    @Test
    void test_host_callables_optional_args_all_set_deliver_both() {
        // `callback(x, y=2, z=3)` supplies both -> 500 + 20 + 3 = 523.
        assertEquals(
                List.of(523L),
                Fns.call_callback_with_optional_args_all_set(optional_args_cb, 5L));
    }
}
