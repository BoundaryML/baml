//! Java sdk-test crate. A `javac` and a `junit` gate per fixture, declared
//! below and expanded by `sdk_test_harness_runner::java::test_suite!`.
//!
//! A gate marked `later` is emitted `#[ignore]`d: the generated Java API is
//! not complete enough for that fixture yet. Flipping it to `on` is the signal
//! that the fixture's parity tests are expected to pass — see
//! `sdks/agent-docs/bridge-ref/ref-java-state-of-completeness.md`.
//!
//! `llm_functions` qualifies for a green `junit` despite calling the engine:
//! its suite is fully deterministic offline with no live keys — streaming runs
//! against the in-process replay server (keyless SSE recordings) and
//! FunctionSpec prompt/parse coverage is offline. `docstrings_etc` qualifies
//! trivially: it only reads the generated `.java` source and asserts the
//! rolled-up Javadoc, making no engine calls at all.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::java::test_suite! {
    fixture docstrings_etc   { javac: on,    junit: on    }
    fixture function_calls   { javac: on,    junit: on    }
    fixture llm_functions    { javac: on,    junit: on    }
    fixture type_shapes      { javac: on,    junit: on    }
    fixture unsupported_only { javac: later, junit: later }
}
