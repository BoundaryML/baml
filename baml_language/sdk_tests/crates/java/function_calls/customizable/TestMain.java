// Smoke tests for plain (non-LLM) expression functions.
//
// Port of python_pydantic2/function_calls/customizable/test_main.py — same
// test names, cases, inputs, assertions. Free functions live on the
// root-namespace holder `baml_sdk.Fns` (PreserveCase names).

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.Fns;
import org.junit.jupiter.api.Test;

class TestMain {

    @Test
    void test_main_hello_world_returns_literal() {
        assertEquals("hello world", Fns.hello_world());
    }

    @Test
    void test_main_single_required_arg_round_trips() {
        // The next step up from the nullary case: one required positional
        // argument round-trips through the engine unchanged.
        assertEquals("hi", Fns.single_required_arg("hi"));
    }
}
