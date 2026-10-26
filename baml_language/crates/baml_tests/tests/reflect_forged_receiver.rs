//! A `reflect.Package` whose `_inner` is not the runtime's own package object
//! can be written (`_inner` is `unknown`), but no program can mean it: the
//! runtime's readers find a payload that is not theirs and report a malformed
//! argument, an internal error, never a catchable throw a program could
//! handle as if the package existed. Artifact misuse through well-typed
//! values stays catchable (see the `reflect` corpus).

use baml_tests::baml_test;

#[tokio::test]
async fn a_forged_package_receiver_is_an_internal_error() {
    let output = baml_test!(
        r##"
        function main() -> string {
            let p = reflect.Package { _inner: 1 };
            p.get_class("X").name()
        }
        "##
    );

    let Err(bex_engine::EngineError::TracedVmInternalError {
        source: bex_vm::errors::VmInternalError::MalformedArgument { message },
        ..
    }) = output.result
    else {
        panic!(
            "expected the forged receiver as an internal error, got: {:?}",
            output.result
        );
    };
    assert!(
        message.contains("reflect.Package is not initialized"),
        "the reader names what it found, got {message:?}"
    );
}
