//! Fused instructions: a program run with them behaves exactly as the same
//! program run without them, down to the traceback of a panic raised from
//! inside one.
//!
//! The emitter fuses at `OptLevel::Two` only, where other passes change the
//! program too. To compare like with like, most tests here take the program
//! compiled at `OptLevel::One`, where nothing is fused, and fuse every
//! function of a second copy with the same pass.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use baml_tests::{
    engine::{OptLevel, function_index, run_compiled},
    stdlib_prefix::compile_source_with_opt,
};
use bex_external_types::BexExternalValue;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{Instruction, IntDst, IntLhs, IntRhs, Object, Program};
use indexmap::IndexMap;

/// `source` compiled without fused instructions, then fused.
fn fused(source: &str) -> Program {
    let mut program = compile_source_with_opt(source, OptLevel::One);
    for object in &mut program.objects.0 {
        if let Object::Function(function) = object {
            let function = &mut **function;
            baml_compiler2_emit::fuse::fuse(&mut function.bytecode, &function.local_names);
        }
    }
    program
}

fn instructions(program: &Program, function: &str) -> Vec<Instruction> {
    let index = function_index(program, &format!("user.{function}"))
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let Some(Object::Function(function)) = program.objects.get(index) else {
        panic!("`{function}` is not a function")
    };
    function.bytecode.instructions.clone()
}

fn is_fused(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::IntArith { .. }
            | Instruction::IntCmpJump { .. }
            | Instruction::LoadVarField { .. }
            | Instruction::LoadVarElement { .. }
            | Instruction::MoveVar { .. }
    )
}

/// `main`'s value, or its error's traceback.
async fn outcome(program: Program) -> Result<BexExternalValue, String> {
    run_compiled(program, "main", IndexMap::new(), false)
        .await
        .result
        .map_err(|error| error.to_string())
}

/// The outcome of `main`, which must be the same unfused, fused, and fully
/// optimized.
async fn same_outcome(source: &str) -> Result<BexExternalValue, String> {
    let plain = outcome(compile_source_with_opt(source, OptLevel::One)).await;
    assert_eq!(outcome(fused(source)).await, plain, "fused at O1");
    assert_eq!(
        outcome(compile_source_with_opt(source, OptLevel::Two)).await,
        plain,
        "O2"
    );
    plain
}

const SHAPES: &str = r#"
class Point {
  x: int
  y: int
}

function mix(a: int, b: int, items: int[], p: Point) -> int {
  let total = a + b;
  total = total * 3;
  let i = 0;
  while (i < items.length()) {
    total += items[i] - a % 7;
    if (items[i] / 2 >= b) {
      total -= 1;
    }
    if (total * 2 > items[i] + 40) {
      total = total - items[i] * 2;
    }
    i += 1;
  }
  for (let j = 10; j > 0; j -= 3) {
    if (j == 4) {
      total += p.x;
    } else if (j != 7) {
      total += p.y;
    }
    if (j <= 1) {
      total = 0 - total;
    }
  }
  let low = a;
  let high = b;
  for (let k = 0; k < 6; k += 1) {
    let next = high;
    high = low + high;
    low = next;
  }
  total = total + p.x * p.y;
  low * 1000 + high + total
}

function main() -> int {
  mix(17, 5, [4, 9, 16, 25, 36], Point { x: 3, y: -2 })
}
"#;

#[tokio::test]
async fn every_fused_form_computes_what_its_run_computes() {
    let program = fused(SHAPES);
    let mix = instructions(&program, "mix");
    let has = |what: &str, test: &dyn Fn(&Instruction) -> bool| {
        assert!(mix.iter().any(test), "no {what} in {mix:#?}");
    };
    // Each operand source and each result destination is exercised.
    has("arithmetic on two locals", &|i| {
        matches!(
            i,
            Instruction::IntArith {
                lhs: IntLhs::Var(_),
                rhs: IntRhs::Var(_),
                ..
            }
        )
    });
    has("arithmetic on a local and an immediate", &|i| {
        matches!(
            i,
            Instruction::IntArith {
                lhs: IntLhs::Var(_),
                rhs: IntRhs::Imm(_),
                ..
            }
        )
    });
    has("arithmetic with its left operand on the stack", &|i| {
        matches!(
            i,
            Instruction::IntArith {
                lhs: IntLhs::Stack,
                rhs: IntRhs::Var(_) | IntRhs::Imm(_),
                ..
            }
        )
    });
    has("arithmetic storing its result", &|i| {
        matches!(
            i,
            Instruction::IntArith {
                dst: IntDst::Var(_),
                ..
            }
        )
    });
    has("arithmetic pushing its result", &|i| {
        matches!(
            i,
            Instruction::IntArith {
                dst: IntDst::Stack,
                ..
            }
        )
    });
    has("compare-and-branch on locals", &|i| {
        matches!(
            i,
            Instruction::IntCmpJump {
                lhs: IntLhs::Var(_),
                ..
            }
        )
    });
    has("compare-and-branch on the stack", &|i| {
        matches!(
            i,
            Instruction::IntCmpJump {
                lhs: IntLhs::Stack,
                ..
            }
        )
    });
    has("field read", &|i| {
        matches!(i, Instruction::LoadVarField { .. })
    });
    has("element read", &|i| {
        matches!(i, Instruction::LoadVarElement { .. })
    });
    has("local move", &|i| matches!(i, Instruction::MoveVar { .. }));

    let value = same_outcome(SHAPES).await;
    assert!(matches!(value, Ok(BexExternalValue::Int(_))), "{value:?}");
}

/// A panic raised inside a fused instruction reports the line, and the
/// frames, the unfused instruction reports. `step` is inlined into `main` at
/// `O2`, so its frame is one the VM rebuilds from the inline table.
#[tokio::test]
async fn a_panic_in_a_fused_instruction_keeps_its_traceback() {
    for (body, panic) in [
        ("limit + grow", "IntegerOverflow"),
        ("limit * (grow + grow)", "IntegerOverflow"),
        ("limit - (0 - limit) - grow - grow", "IntegerOverflow"),
        ("limit / (grow - 1)", "DivisionByZero"),
        ("limit % (grow - 1)", "DivisionByZero"),
        ("items[grow + 2]", "IndexOutOfBounds"),
    ] {
        let source = format!(
            r#"
function step(limit: int, grow: int, items: int[]) -> int {{
  let before = grow + 1;
  let result =
    {body};
  result + before
}}

function main() -> int {{
  let items = [1, 2, 3];
  let total = 0;
  for (let grow = 0; grow < 2; grow += 1) {{
    total += step(4611686018427387903, grow, items);
  }}
  total
}}
"#
        );
        let program = fused(&source);
        let step = instructions(&program, "step");
        assert!(step.iter().any(is_fused), "{body}: {step:?}");
        let error = same_outcome(&source).await.unwrap_err();
        assert!(error.contains(panic), "{body}: {error}");
        assert!(
            error.contains("line 13, in user.main") && error.contains(", in user.step"),
            "{body}: {error}"
        );
    }
}

/// A handler's range, and a `defer` body's shielded range, cover a fused
/// instruction exactly when they covered its run.
#[tokio::test]
async fn handlers_and_deferred_bodies_cover_fused_instructions() {
    let source = r#"
function guarded(start: int, rounds: int) -> int {
  let caught = 0;
  let value = start;
  let cleanup = 0;
  for (let i = 0; i < rounds; i += 1) {
    defer {
      cleanup = cleanup + i * 2;
    }
    value = (value * 4611686018427387 + i) catch (e) {
      baml.panics.IntegerOverflow => {
        caught += 1;
        i + 1
      }
    };
  }
  value * 100 + caught * 10 + cleanup
}

function main() -> int {
  guarded(3, 4)
}
"#;
    let program = fused(source);
    let guarded = instructions(&program, "guarded");
    assert!(guarded.iter().any(is_fused), "{guarded:?}");
    let value = same_outcome(source).await;
    assert!(matches!(value, Ok(BexExternalValue::Int(_))), "{value:?}");
}

/// A fused compare-and-branch is a yield point, like the branch it replaced:
/// a loop made only of fused instructions still parks when asked to, and
/// resumes where it left off.
#[test]
fn a_fused_loop_yields_and_resumes() {
    const ROUNDS: i64 = 20_000;
    let source = format!(
        r#"
function main() -> int {{
  let total = 0;
  for (let i = 0; i < {ROUNDS}; i += 1) {{
    total += i;
  }}
  total
}}
"#
    );
    let program = compile_source_with_opt(&source, OptLevel::Two);
    let main = instructions(&program, "main");
    assert!(
        main.iter().all(|instruction| is_fused(instruction)
            || matches!(instruction, Instruction::Return | Instruction::LoadVar(_))
            || matches!(
                instruction,
                Instruction::LoadConst(_) | Instruction::StoreVar(_)
            )
            || matches!(instruction, Instruction::StoreVar2(..))),
        "the loop is fused throughout: {main:?}"
    );
    assert!(
        main.iter()
            .any(|instruction| matches!(instruction, Instruction::IntCmpJump { .. })),
        "{main:?}"
    );

    let index = function_index(&program, "user.main").unwrap();
    let park = Arc::new(AtomicBool::new(true));
    let mut vm = BexVm::from_program(program, Arc::clone(&park)).expect("from_program");
    vm.early_yield = bex_vm_types::EarlyYieldCheck::with_interval(Arc::clone(&park), 1 << 8);
    let function = vm.heap.compile_time_ptr(index);
    vm.set_entry_point(function, &[]);

    let mut yields = 0;
    let value = loop {
        match vm.exec().expect("exec") {
            VmExecState::EarlyYield => {
                yields += 1;
                // Keep asking for a few polls, then let it finish.
                if yields == 3 {
                    park.store(false, Ordering::Release);
                }
                assert!(yields < 1000, "the loop never finished");
            }
            VmExecState::Complete(value) => break value,
            other => panic!("unexpected state: {other:?}"),
        }
    };
    assert_eq!(yields, 3);
    assert_eq!(value.as_int(), Some(ROUNDS * (ROUNDS - 1) / 2));
}

/// Lower levels emit one instruction per operation: what the debugger steps
/// through and what instruction-level tests count.
#[test]
fn only_o2_fuses() {
    for (opt, expected) in [
        (OptLevel::Zero, false),
        (OptLevel::One, false),
        (OptLevel::Two, true),
    ] {
        let program = compile_source_with_opt(SHAPES, opt);
        assert_eq!(
            instructions(&program, "mix").iter().any(is_fused),
            expected,
            "{opt:?}"
        );
    }
}
