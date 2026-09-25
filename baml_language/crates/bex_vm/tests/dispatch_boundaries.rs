use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

fn run(source: &str) -> i64 {
    let program = compile_source(source);
    let entry = program.function_index("user.main").expect("main");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
    let entry = vm.heap.compile_time_ptr(entry);
    vm.set_entry_point(entry, &[]);
    match vm.exec().expect("execution succeeds") {
        VmExecState::Complete(value) => value.as_int().expect("integer result"),
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn indirect_calls_and_native_callbacks_resume_the_right_instruction_stream() {
    assert_eq!(
        run(r#"
            function twice(n: int) -> int { n * 2 }
            function main() -> int {
                let bias = 10;
                let captured = (n: int) -> int { twice(n) + bias };
                let values = [1, 2, 3].map(captured);
                let indirect = twice;
                values[0] + values[1] + values[2] + indirect(4)
            }
        "#),
        50,
    );
}

#[test]
fn explicit_throws_resume_same_frame_and_caller_handlers() {
    assert_eq!(
        run(r#"
            function fail() -> int { throw "cross-frame" }
            function nested() -> int { 100 + fail() }
            function main() -> int {
                let local = { throw "same-frame"; 0 } catch (e) { _ => 7 };
                let caller = nested() catch (e) { _ => 11 };
                local * 100 + caller
            }
        "#),
        711,
    );
}

#[test]
fn panic_rethrows_resume_same_frame_and_caller_handlers() {
    assert_eq!(
        run(r#"
            function divide(n: int) -> int { 10 / n }
            function pass_panic() -> int {
                divide(0) catch (e) { _ => 1000 }
            }
            function main() -> int {
                let local = {
                    divide(0) catch (e) { _ => 2000 }
                } catch (e) { baml.panics.DivisionByZero => 7 };
                let caller = pass_panic()
                    catch (e) { baml.panics.DivisionByZero => 11 };
                local * 100 + caller
            }
        "#),
        711,
    );
}
