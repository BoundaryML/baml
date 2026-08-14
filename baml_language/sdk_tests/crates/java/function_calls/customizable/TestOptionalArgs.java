// Optional-argument matrix (root `optional_args_probe` + `OptBox` methods).
//
// Port of python_pydantic2/function_calls/customizable/test_optional_args.py —
// same test names, cases, inputs, assertion strength.
//
// Optional args use the AWS-SDK-v2-style trailing configurator overload
// (ref-java-codegen-conventions.md):
//   * `Fns.optional_args_probe(1L)`            — omits every optional; the
//     engine evaluates BAML defaults. This is the UNSET/omitted state.
//   * `Fns.optional_args_probe(1L, o -> {})`   — an empty configurator is the
//     SAME omitted state (there is no UNSET sentinel value; UNSET == "the
//     setter was never called").
//   * `Fns.optional_args_probe(1L, o -> o.opt1(null))` — sends an EXPLICIT
//     BAML `null`, distinct from omission.
// This preserves the omit-vs-null tri-state without a sentinel type.
//
// BAML `int?[]` -> `java.util.List<Long>` with nullable elements, so the
// expected lists use `Arrays.asList(...)` (which, unlike `List.of`, permits
// null elements).

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.Fns;
import baml_sdk.OptBox;
import java.util.Arrays;
import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class TestOptionalArgs {

    @Test
    void test_optional_args_runtime_matrix() {
        assertEquals(Arrays.asList(1L, 5L, 99L), Fns.optional_args_probe(1L));
        // opt1=UNSET, opt2=UNSET -> both omitted (empty configurator).
        assertEquals(Arrays.asList(1L, 5L, 99L), Fns.optional_args_probe(1L, o -> {}));
        // arg0=1 kwarg -> positional in Java.
        assertEquals(Arrays.asList(1L, 5L, 99L), Fns.optional_args_probe(1L));
        // opt1=None -> explicit null.
        assertEquals(Arrays.asList(1L, null, 99L), Fns.optional_args_probe(1L, o -> o.opt1(null)));
        assertEquals(Arrays.asList(1L, 8L, 99L), Fns.optional_args_probe(1L, o -> o.opt1(8L)));
        assertEquals(Arrays.asList(1L, 5L, null), Fns.optional_args_probe(1L, o -> o.opt2(null)));
        assertEquals(Arrays.asList(1L, 5L, 9L), Fns.optional_args_probe(1L, o -> o.opt2(9L)));
        assertEquals(
                Arrays.asList(1L, null, null),
                Fns.optional_args_probe(1L, o -> {
                    o.opt1(null);
                    o.opt2(null);
                }));
        assertEquals(
                Arrays.asList(1L, 8L, 9L),
                Fns.optional_args_probe(1L, o -> {
                    o.opt1(8L);
                    o.opt2(9L);
                }));
    }

    @Test
    void test_optional_args_python_unset_and_none_differ_in_one_call() {
        // UNSET means "omit this argument"; None means "pass an explicit null".
        // The two must stay distinct within a single call.
        //
        // java-port note: Python asserts `baml.UNSET is not None` against a
        // sentinel value. Java's configurator has no sentinel — UNSET is
        // expressed structurally by NOT calling the setter — so the identity
        // check has no direct analog. The two behavioral assertions below carry
        // the omit-vs-null distinction within a single call.
        // opt1=UNSET (omit), opt2=None (explicit null).
        assertEquals(Arrays.asList(1L, 5L, null), Fns.optional_args_probe(1L, o -> o.opt2(null)));
        // opt1=None (explicit null), opt2=UNSET (omit).
        assertEquals(Arrays.asList(1L, null, 99L), Fns.optional_args_probe(1L, o -> o.opt1(null)));
    }

    @Test
    void test_optional_args_async_samples() {
        // await fn_async(...) -> Fns.fn_async(...).join()
        assertEquals(Arrays.asList(1L, 5L, 99L), Fns.optional_args_probe_async(1L).join());
        assertEquals(
                Arrays.asList(1L, null, 99L),
                Fns.optional_args_probe_async(1L, o -> o.opt1(null)).join());
        assertEquals(
                Arrays.asList(1L, 5L, 9L),
                Fns.optional_args_probe_async(1L, o -> o.opt2(9L)).join());
    }

    @Test
    void test_optional_args_opt_box_method_matrix() {
        OptBox box = OptBox.make(10L);
        assertEquals(17L, box.base());

        OptBox box2 = OptBox.make(10L, o -> o.opt1(0L));
        assertEquals(10L, box2.base());
        assertEquals(Arrays.asList(10L, 1L, 5L), box2.probe(1L));
        assertEquals(Arrays.asList(10L, 1L, 8L), box2.probe(1L, o -> o.opt1(8L)));
    }

    @Disabled(
            "java-port note: Python's three negative cases all bypass static "
                    + "typing via dict-splat / duplicate kwargs, so they fail only at "
                    + "runtime. In Java each is a COMPILE error, not a runtime one: "
                    + "`optional_args_probe(1L, o -> o.opt3(1L))` (no such setter), "
                    + "`optional_args_probe()` (missing required arg0), and a duplicated "
                    + "arg0 all fail javac. The compile-surface analog is documented in "
                    + "OptionalArgsStatic.java, mirroring optional_args_static.py.")
    @Test
    void test_optional_args_negative_runtime_cases_reject() {
        // Intentionally empty — see @Disabled reason and OptionalArgsStatic.java.
    }
}
