use baml_tests::{
    engine::{OptLevel, function_index, run_test},
    stdlib_prefix::compile_source_with_opt,
};
use bex_external_types::BexExternalValue;
use bex_vm_types::{Instruction, Object, Program};
use indexmap::IndexMap;

const SOURCE: &str = r#"
class Point {
    x: int
    y: int
}
class Segment {
    from: Point
    to: Point
}
class Handle {
    tag: int
    function cleanup(self) -> void { }
}

function basic(n: int) -> int {
    let p = Point { x: n, y: n + 1 }
    p.x * p.x + p.y * p.y
}
function fresh_per_iteration(n: int) -> int {
    let acc = 0
    let i = 0
    while (i < n) {
        let p = Point { x: i, y: i + 1 }
        acc = acc + p.x * p.x + p.y * p.y
        i = i + 1
    }
    acc
}
function mutated(n: int) -> int {
    let p = Point { x: n, y: 0 }
    p.y = p.x + 1
    p.x = p.y * 2
    p.x + p.y
}
function conditional(flag: bool, n: int) -> int {
    let p = if (flag) { Point { x: n, y: 1 } } else { Point { x: 1, y: n } }
    p.x - p.y
}
function conditionally_rebuilt(n: int) -> int {
    let p = Point { x: 0, y: 0 }
    let i = 0
    while (i < n) {
        if (i % 2 == 0) { p = Point { x: p.x + i, y: p.y } } else { p.y = p.y + 1 }
        i = i + 1
    }
    p.x * 100 + p.y
}
function rebuilt_from_itself(n: int) -> int {
    let p = Point { x: n, y: n + 1 }
    p = Point { x: p.y, y: p.x }
    p.x * 10 + p.y
}
function aliased(n: int) -> int {
    let p = Point { x: n, y: 0 }
    let q = p
    q.y = 7
    p.x + p.y
}
function nested(n: int) -> int {
    let s = Segment { from: Point { x: n, y: 1 }, to: Point { x: 2, y: n } }
    s.from.x + s.to.y
}
function shared_by_two_fields(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let s = Segment { from: p, to: p }
    s.from.y = 5
    s.to.y + p.y
}
function spread(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let q = Point { ...p, y: 2 }
    q.x + q.y + p.y
}
function panic_between_writes(divisor: int) -> int {
    let p = Point { x: 1, y: 2 }
    {
        p.x = 10
        p.y = 100 / divisor
        p.x + p.y
    } catch (e) { baml.panics.DivisionByZero => p.x * 1000 + p.y }
}
function panic_in_rebuild(xs: int[]) -> int {
    let p = Point { x: 1, y: 2 }
    {
        p = Point { x: 10, y: xs[5] }
        p.x + p.y
    } catch (e) { baml.panics.IndexOutOfBounds => p.x * 1000 + p.y }
}

// Each calls itself on a path never taken, which keeps its calls from being
// inlined: an instance handed to a real call is the subject below.
function read_x(p: Point, again: bool = false) -> int {
    if (again) { return read_x(p) }
    p.x
}
function read_from_y(s: Segment, again: bool = false) -> int {
    if (again) { return read_from_y(s) }
    s.from.y
}
function passed(n: int) -> int {
    let p = Point { x: n, y: 1 }
    read_x(p) + p.y
}
function returned(n: int) -> Point {
    let p = Point { x: n, y: 1 }
    p.y = 2
    p
}
function stored_in_array(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let ps = [p]
    p.y = 4
    ps[0].y + p.x
}
function stored_in_escaping_object(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let s = Segment { from: p, to: p }
    p.y = 9
    read_from_y(s)
}
// Calls itself on a path never taken, so the closure handed to it stays a
// closure: one called where it is made is inlined, and what it captured is
// then no longer captured.
function call_later(f: () -> int, again: bool = false) -> int {
    if (again) { return call_later(f) }
    f()
}
function captured(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let read = () => { p.x }
    p.x = 5
    call_later(read)
}
function stale_alias(n: int) -> int {
    let p = Point { x: n, y: 1 }
    let old = p
    p = Point { x: 100, y: 2 }
    old.x + p.x
}
function finalized(n: int) -> int {
    let h = Handle { tag: n }
    h.tag
}

function verify_all() -> bool {
    basic(3) == 25 && fresh_per_iteration(4) == 44 && mutated(3) == 12
        && conditional(true, 5) == 4 && conditional(false, 5) == -4
        && conditionally_rebuilt(5) == 602 && rebuilt_from_itself(3) == 43
        && aliased(3) == 10 && nested(3) == 6 && shared_by_two_fields(3) == 10
        && spread(3) == 6
        && panic_between_writes(4) == 35 && panic_between_writes(0) == 10002
        && panic_in_rebuild([0, 1, 2, 3, 4, 5]) == 15 && panic_in_rebuild([1]) == 1002
        && passed(3) == 4 && returned(3).x + returned(3).y == 5
        && stored_in_array(3) == 7 && stored_in_escaping_object(3) == 9
        && captured(3) == 5 && stale_alias(3) == 103 && finalized(3) == 3
}
"#;

fn allocations(program: &Program, name: &str) -> usize {
    let index = function_index(program, &format!("user.{name}")).unwrap();
    let Some(Object::Function(function)) = program.objects.get(index) else {
        panic!("expected function")
    };
    function
        .bytecode
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction,
                Instruction::AllocInstance { .. } | Instruction::InitInstance(_)
            )
        })
        .count()
}

#[tokio::test]
async fn replaced_instances_behave_the_same_at_each_level() {
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let output = run_test(SOURCE, "verify_all", IndexMap::new(), opt).await;
        assert_eq!(output.result, Ok(BexExternalValue::Bool(true)), "{opt:?}");
    }
}

#[test]
fn instances_that_stay_in_their_function_are_not_allocated() {
    let program = compile_source_with_opt(SOURCE, OptLevel::Two);
    for name in [
        "basic",
        "fresh_per_iteration",
        "mutated",
        "conditional",
        "conditionally_rebuilt",
        "rebuilt_from_itself",
        "aliased",
        "nested",
        "shared_by_two_fields",
        "spread",
        "panic_between_writes",
        "panic_in_rebuild",
    ] {
        assert_eq!(allocations(&program, name), 0, "{name}");
    }
}

#[test]
fn instances_something_else_can_see_stay_allocated() {
    let program = compile_source_with_opt(SOURCE, OptLevel::Two);
    for (name, expected) in [
        ("passed", 1),
        ("returned", 1),
        ("stored_in_array", 1),
        // The segment is handed to a call, so it and the point it holds stay.
        ("stored_in_escaping_object", 2),
        ("captured", 1),
        // `old` still names the first point after `p` is rebuilt.
        ("stale_alias", 2),
        // The collector runs `cleanup` on the instance.
        ("finalized", 1),
    ] {
        assert_eq!(allocations(&program, name), expected, "{name}");
    }
}

#[test]
fn replacement_needs_the_full_optimization_level() {
    let program = compile_source_with_opt(SOURCE, OptLevel::One);
    assert_eq!(allocations(&program, "basic"), 1);
}
