import static org.junit.jupiter.api.Assertions.*;
import baml_sdk.BamlOptions;
import baml_sdk.Invocation;
import baml_sdk.Target;
import baml_sdk.Fns;
import baml_sdk.baml.spawn.CancelToken;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.HashMap;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Nested;

class TestInvocation {
    @Nested class invocation_options {
        @Test void test_four_call_forms() {
            var options = BamlOptions.builder().timeoutMs(1000).build();
            assertEquals(List.of(1L,5L,99L), Fns.optional_args_probe(1));
            assertEquals(List.of(1L,7L,99L), Fns.optional_args_probe(1, app -> app.opt1(7L)));
            assertEquals(List.of(1L,5L,99L), Fns.optional_args_probe(1, options));
            assertEquals(List.of(1L,7L,99L), Fns.optional_args_probe(1, app -> app.opt1(7L), options));
        }
        @Test void test_four_call_forms_async() {
            var options = BamlOptions.builder().timeoutMs(1000).build();
            assertEquals(List.of(1L,5L,99L), Fns.optional_args_probe_async(1).join());
            assertEquals(List.of(1L,7L,99L), Fns.optional_args_probe_async(1, app -> app.opt1(7L)).join());
            assertEquals(List.of(1L,5L,99L), Fns.optional_args_probe_async(1, options).join());
            assertEquals(List.of(1L,7L,99L), Fns.optional_args_probe_async(1, app -> app.opt1(7L), options).join());
        }
        @Test void test_empty_controls() { assertEquals(Fns.hello_world(), Fns.hello_world(BamlOptions.empty())); }
        @Test void test_omitted_argument_is_not_null() { assertEquals(Arrays.asList(1L,null,99L), Fns.optional_args_probe(1, app -> app.opt1(null), BamlOptions.empty())); }
        @Test void test_invalid_timeout_rejected() { for (long timeout : new long[]{-1,2147483648L}) assertThrows(IllegalArgumentException.class, () -> Fns.hello_world(BamlOptions.builder().timeoutMs(timeout).build())); }
        @Test void test_timeout_upper_bound_accepted() { assertEquals("hello world", Fns.hello_world(BamlOptions.builder().timeoutMs(2147483647L).build())); }
    }
    @Nested class invocation_surfaces {
        @Test void test_methods_accept_controls() {
            var box = baml_sdk.OptBox.make(1, BamlOptions.empty());
            assertEquals(List.of(8L,2L,5L), box.probe(2, BamlOptions.empty()));
        }
        @Test void test_returned_callable_accepts_controls() {
            var add = baml_sdk.host_callable_tests.Fns.make_adder(3);
            assertEquals(7L, add.call(4L, BamlOptions.empty()));
            assertEquals(7L, add.callAsync(4L, BamlOptions.empty()).join());
            assertEquals(8L, baml_sdk.Baml.invoke(
                Target.named("user.host_callable_tests.call_int_callback"),
                Map.of("callback", add, "x", 5L), null, BamlOptions.empty()));
        }
        @Test void test_dynamic_name_and_handle_accept_controls() {
            assertEquals("hello world", baml_sdk.Baml.invoke(Target.named("user.hello_world"), Map.of(), null, BamlOptions.empty()));
            var add = baml_sdk.host_callable_tests.Fns.make_adder(3);
            assertEquals(7L, baml_sdk.Baml.invoke(Target.callable(add), Map.of("value",4L), null, BamlOptions.empty()));
            assertThrows(IllegalArgumentException.class, () -> Target.callable((java.util.function.Function<Long,Long>) value -> value));
            assertThrows(IllegalArgumentException.class, () -> baml_sdk.Baml.invoke(Target.callable(add), Map.of("value",4L), baml_bridge.BamlTypes.of("T", baml_bridge.BamlType.INT), BamlOptions.empty()));
        }
    }
    @Nested class invocation_lifecycle {
        @Test void test_pre_cancelled_call_does_not_enter_callback() {
            var token = CancelToken.new$(); token.cancel();
            assertThrows(baml_bridge.BamlPanic.class, () -> baml_sdk.host_callable_tests.Fns.call_int_callback(value -> { fail("must not enter"); return value; }, 1, BamlOptions.builder().cancel(token).build()));
        }
        @Test void test_zero_timeout_does_not_enter_callback() {
            assertThrows(baml_bridge.BamlPanic.class, () -> baml_sdk.host_callable_tests.Fns.call_int_callback(value -> { fail("must not enter"); return value; }, 1, BamlOptions.builder().timeoutMs(0).build()));
        }
        @Test void test_composite_token_observes_every_source() {
            for (int index = 0; index < 2; index++) { var sources = List.of(CancelToken.new$(),CancelToken.new$()); var composite = CancelToken.any(sources); sources.get(index).cancel(); assertTrue(composite.is_cancelled()); assertFalse(sources.get(1-index).is_cancelled()); }
        }
        @Test void test_reservation_is_single_use() {
            var reserved = baml_sdk.vendor.trace.Fns.hidden().reserve(); var options = BamlOptions.builder().trace(reserved).build();
            assertEquals("hello world", Fns.hello_world(options)); assertThrows(IllegalArgumentException.class, () -> Fns.hello_world(options));
        }
        @Test void test_failed_admission_does_not_consume_reservation() {
            var reserved = baml_sdk.vendor.trace.Fns.hidden().reserve();
            assertThrows(baml_bridge.BamlPanic.class, () -> Fns.hello_world(BamlOptions.builder().trace(reserved).timeoutMs(0).build()));
            assertEquals("hello world", Fns.hello_world(BamlOptions.builder().trace(reserved).build()));
        }
        @Test void test_retained_effective_token_observes_late_parent_cancellation() {
            var source = CancelToken.new$(); var retained = new AtomicReference<Invocation>();
            baml_sdk.host_callable_tests.Fns.call_int_callback(value -> { retained.set(Invocation.current().orElseThrow()); return value; }, 1, BamlOptions.builder().cancel(source).build());
            assertTrue(Invocation.current().isEmpty()); source.cancel(); assertTrue(retained.get().cancel().is_cancelled());
            assertThrows(baml_bridge.BamlPanic.class, () -> retained.get().run(Fns::hello_world));
        }
    }
    @Nested class invocation_inheritance {
        @Test void test_multi_layer_context_patch_inherits_and_restores() {
            var outer = baml_sdk.vendor.trace.Fns.context(app -> app.distinct_id("A").metadata(Map.of("outer",new baml_bridge.Union4.Arm1<String,Long,Double,Boolean>(1L),"remove",new baml_bridge.Union4.Arm0<String,Long,Double,Boolean>("old"),"shared",new baml_bridge.Union4.Arm0<String,Long,Double,Boolean>("A"))));
            var metadata = new HashMap<String,baml_bridge.Union4<String,Long,Double,Boolean>>(); metadata.put("inner",new baml_bridge.Union4.Arm1<>(2L)); metadata.put("remove",null); metadata.put("shared",new baml_bridge.Union4.Arm0<>("C"));
            var patch = baml_sdk.vendor.trace.Fns.context(app -> app.distinct_id("C").metadata(metadata));
            for (int empty = 0; empty < 4; empty++) {
                final int controls = empty;
                baml_sdk.host_callable_tests.Fns.call_int_callback(a -> {
                    assertEquals("A", baml_sdk.vendor.trace.Fns.current_context().distinct_id());
                    java.util.function.Function<Long,Long> layerB = b -> {
                        assertEquals("A", baml_sdk.vendor.trace.Fns.current_context().distinct_id());
                        baml_sdk.host_callable_tests.Fns.call_int_callback(c -> {
                            java.util.function.Function<Long,Long> layerD = d -> {
                                var current = baml_sdk.vendor.trace.Fns.current_context(); assertEquals("C",current.distinct_id()); assertEquals(Map.of("outer",new baml_bridge.Union4.Arm1<String,Long,Double,Boolean>(1L),"inner",new baml_bridge.Union4.Arm1<String,Long,Double,Boolean>(2L),"shared",new baml_bridge.Union4.Arm0<String,Long,Double,Boolean>("C")), current.metadata());
                                var active = Invocation.current().orElseThrow();
                                assertEquals("C", CompletableFuture.completedFuture(1L).thenApplyAsync(active.wrapFunction(value -> baml_sdk.vendor.trace.Fns.current_context().distinct_id())).join());
                                return d;
                            };
                            if ((controls & 2) == 0) baml_sdk.host_callable_tests.Fns.call_int_callback(layerD,c); else baml_sdk.host_callable_tests.Fns.call_int_callback(layerD,c,BamlOptions.empty()); return c;
                        }, b, BamlOptions.builder().trace(patch).build());
                        assertEquals("A", baml_sdk.vendor.trace.Fns.current_context().distinct_id()); return b;
                    };
                    if ((controls & 1) == 0) baml_sdk.host_callable_tests.Fns.call_int_callback(layerB,a); else baml_sdk.host_callable_tests.Fns.call_int_callback(layerB,a,BamlOptions.empty()); return a;
                }, 1, BamlOptions.builder().trace(outer).build());
            }
            assertTrue(Invocation.current().isEmpty()); assertNull(baml_sdk.vendor.trace.Fns.current_context().distinct_id());
        }
    }
}
