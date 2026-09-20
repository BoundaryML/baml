//! Acceptance tests: a run that is moved to a fresh heap and VM at its yields
//! produces the same result as an uninterrupted run.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::sync::atomic::Ordering;

use common::Harness;

/// Nested calls, a `while` loop, a `for` loop over an array, class instances,
/// arrays, maps, strings, floats, an enum, and a sys-op that returns a value.
const ORDER_PROGRAM: &str = r#"
enum Priority {
    Low
    High
}

class Item {
    name string
    qty int
    tags string[]
}

class Order {
    id int
    items Item[]
    notes map<string, string>
    weight float
    priority Priority
}

class Report {
    summary string
    order Order
    answer string
    count int
}

function build_item(i: int) -> Item {
    baml.io.println("build item " + i.to_string());
    Item { name: "item-" + i.to_string(), qty: i * 2, tags: ["t" + i.to_string(), "common"] }
}

function build_order(n: int) -> Order {
    let items: Item[] = [];
    let notes: map<string, string> = {};
    let i = 0;
    while (i < n) {
        let item = build_item(i);
        items.push(item);
        notes["k" + i.to_string()] = item.name;
        i += 1;
    }
    Order { id: 7, items: items, notes: notes, weight: 1.5, priority: Priority.High }
}

function summarize(order: Order) -> string {
    let out = "";
    for (let item in order.items) {
        baml.io.println("summarize " + item.name);
        out = out + item.name + ":" + item.qty.to_string() + ";";
    }
    out
}

function main() -> Report {
    let order = build_order(3);
    let summary = summarize(order);
    let answer = baml.io.input("name?") catch (e) { _ => "no input" };
    baml.io.println("done");
    Report { summary: summary, order: order, answer: answer, count: order.items.length() }
}
"#;

#[test]
fn nested_calls_loops_and_data_structures_survive_hops() {
    let harness = Harness::new(ORDER_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert!(expected.contains("item-2:4;"), "baseline: {expected}");
    assert!(expected.contains("answer 6 to"), "baseline: {expected}");

    let (actual, hops) = harness.run_hopping("user.main", false, 0);
    assert_eq!(actual, expected);
    // 3 build_item + 3 summarize + input + done.
    assert_eq!(hops, 8);
}

#[test]
fn compressed_snapshots_round_trip() {
    let harness = Harness::new(ORDER_PROGRAM);
    let expected = harness.run_plain("user.main");
    let (actual, hops) = harness.run_hopping("user.main", true, 0);
    assert_eq!(actual, expected);
    assert_eq!(hops, 8);
}

const CLOSURE_PROGRAM: &str = r#"
function make_counter(start: int) -> () -> int throws never {
    let counter = start;
    let bump = () -> int { counter += 1; counter };
    bump
}

function main() -> int[] {
    let bump = make_counter(10);
    let seen: int[] = [];
    let i = 0;
    while (i < 3) {
        baml.io.println("tick");
        seen.push(bump());
        i += 1;
    }
    seen
}
"#;

#[test]
fn closures_keep_their_captured_variables() {
    let harness = Harness::new(CLOSURE_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, "[11, 12, 13]");
    let (actual, hops) = harness.run_hopping("user.main", false, 0);
    assert_eq!(actual, expected);
    assert_eq!(hops, 3);
}

/// The sys-op yields happen inside `risky`, while the `catch` of `main` is
/// active further up the stack. After a restore the throw must still unwind
/// into that handler.
const CATCH_PROGRAM: &str = r#"
function risky(n: int) -> int throws string {
    baml.io.println("risky " + n.to_string());
    if (n > 1) { throw "too big" }
    n * 10
}

function main() -> int[] {
    let results: int[] = [];
    let i = 0;
    while (i < 4) {
        let v = risky(i) catch (e) {
            "too big" => -1,
            _ => -2
        };
        results.push(v);
        i += 1;
    }
    results
}
"#;

#[test]
fn a_catch_block_active_on_the_stack_still_catches_after_a_restore() {
    let harness = Harness::new(CATCH_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, "[0, 10, -1, -1]");
    let (actual, hops) = harness.run_hopping("user.main", false, 0);
    assert_eq!(actual, expected);
    assert_eq!(hops, 4);
}

/// The yield happens inside a catch handler, while the caught value's throw
/// bookkeeping is live in the VM, and the handler then rethrows.
const RETHROW_PROGRAM: &str = r#"
function inner(n: int) -> int throws string {
    if (n > 0) { throw "boom " + n.to_string() }
    n
}

function middle(n: int) -> int throws string {
    inner(n) catch (e) {
        _ => {
            baml.io.println("handling");
            throw e
        }
    }
}

function main() -> string[] {
    let out: string[] = [];
    let i = 0;
    while (i < 3) {
        let r = middle(i).to_string() catch (e) {
            _ => "caught " + e
        };
        out.push(r);
        i += 1;
    }
    out
}
"#;

#[test]
fn a_rethrow_after_a_yield_inside_the_handler_gives_the_same_result() {
    let harness = Harness::new(RETHROW_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, "[\"0\", \"caught boom 1\", \"caught boom 2\"]");
    let (actual, hops) = harness.run_hopping("user.main", false, 0);
    assert_eq!(actual, expected);
    assert_eq!(hops, 2);
}

/// `B` is thrown while `A` is being handled, so the VM records `A` as the
/// cause of `B`. The handler of `B` yields and then rethrows `B`. A rethrow
/// does not recompute the cause: it reads the VM's throw bookkeeping, which
/// therefore has to travel in the snapshot for the outer handler to still see
/// `A` as the root cause.
const CAUSE_CHAIN_PROGRAM: &str = r#"
function fail_a() -> int throws string {
    throw "A"
}

function fail_b() -> int throws string {
    throw "B"
}

function middle() -> int throws string {
    fail_a() catch (e, ctx) {
        _ => {
            fail_b() catch (e2, ctx2) {
                _ => {
                    baml.io.println("handling B");
                    throw e2
                }
            }
        }
    }
}

function main() -> string {
    middle().to_string() catch (e, ctx) {
        _ => {
            match (ctx.root_cause().error) {
                let s: string => e + " caused by " + s,
                _ => "no string cause",
            }
        }
    }
}
"#;

#[test]
fn a_cause_chain_survives_a_hop_between_throw_and_rethrow() {
    let harness = Harness::new(CAUSE_CHAIN_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, "\"B caused by A\"");
    let (actual, hops) = harness.run_hopping("user.main", true, 0);
    assert_eq!(hops, 1);
    assert_eq!(actual, expected);

    // The same with a collection right after the restore: the bookkeeping
    // values are roots of the imported VM and must be forwarded with it.
    harness
        .gc_after_restore
        .set(Some(bex_heap::CollectionLevel::Major));
    let (actual, _) = harness.run_hopping("user.main", true, 0);
    assert_eq!(actual, expected);
}

/// A call with an explicit type argument gives the callee's frame exact
/// runtime-type metadata next to its realized type arguments. Both travel in
/// the snapshot.
const EXPLICIT_TYPE_ARG_PROGRAM: &str = r#"
class Tag {
    name string
}

function pick<T>(x: T) -> T {
    baml.io.println("picking");
    x
}

function main() -> string {
    let tag = pick<Tag>(Tag { name: "a" });
    let word = pick<string>("b");
    tag.name + word
}
"#;

#[test]
fn a_frame_with_explicit_type_arguments_survives_a_hop() {
    let harness = Harness::new(EXPLICIT_TYPE_ARG_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, "\"ab\"");

    // The yield inside `pick<Tag>`: the frame carries its type argument, and
    // the exact-type metadata when the VM recorded one.
    let (vm, _, _) = harness.run_to_sysop("user.main", 0);
    let state = vm.export_thread_state().expect("exports");
    let pick = state.frames.last().expect("a frame");
    assert_eq!(pick.function_name, "user.pick");
    assert_eq!(pick.type_args.len(), 1);
    if let Some(metadata) = &pick.type_metadata {
        assert_eq!(metadata.len(), pick.type_args.len());
    }
    drop(vm);

    let (actual, hops) = harness.run_hopping("user.main", true, 0);
    assert_eq!(hops, 2);
    assert_eq!(actual, expected);
}

/// A compute loop with no sys-ops: the only yields are `EarlyYield`s, taken in
/// the middle of the `while` loop and of the `for` loop (whose iterator is a
/// plain BAML object on the operand stack).
const SPIN_PROGRAM: &str = r#"
function main() -> int {
    let items: int[] = [];
    let i = 0;
    while (i < 3000) {
        items.push(i * 2);
        i += 1;
    }
    let sum = 0;
    for (let x in items) {
        sum += x;
    }
    sum
}
"#;

#[test]
fn a_compute_loop_resumes_from_an_early_yield() {
    let harness = Harness::new(SPIN_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(expected, (0..3000).map(|i| i * 2).sum::<i64>().to_string());

    harness.park.store(true, Ordering::Release);
    let (actual, hops) = harness.run_hopping("user.main", true, 12);
    assert_eq!(actual, expected);
    assert_eq!(hops, 12, "the run is long enough to take every early hop");
}

/// A long caller that calls a tiny function from its loop. The VM checks for
/// an early yield right after a call has pushed the callee's frame and right
/// after a return has popped it. At those yields the last dispatched
/// instruction belongs to the other function, and its program counter is
/// larger than the tiny function's whole code.
const CALL_LOOP_PROGRAM: &str = r#"
function tiny(x: int) -> int {
    x + 1
}

function main() -> int {
    let total = 0;
    let i = 0;
    while (i < 6) {
        let a = tiny(i);
        let b = tiny(a) + tiny(b_seed(i));
        let label = "i=" + i.to_string() + " a=" + a.to_string() + " b=" + b.to_string();
        total = total + a + b + label.length();
        total = tiny(total);
        i += 1;
    }
    total
}

function b_seed(i: int) -> int {
    tiny(i * 3)
}
"#;

#[test]
fn an_early_yield_on_a_call_or_return_boundary_can_be_restored() {
    let harness = Harness::new(CALL_LOOP_PROGRAM);
    let expected = harness.run_plain("user.main");

    // Yield at every check and hop at every yield, so that the run is moved
    // at call boundaries, at return boundaries, and at loop jumps.
    harness.yield_interval.set(1);
    harness.park.store(true, Ordering::Release);
    let (actual, hops) = harness.run_hopping("user.main", true, usize::MAX);
    assert_eq!(actual, expected);
    assert!(hops > 60, "the run hops at every call and return: {hops}");
}

/// At an early yield right after a return, the live program counter is the
/// caller's call site, not the callee's `Return`. The state dump and the
/// worker's `position` event compute the innermost line from it.
#[test]
fn the_live_program_counter_belongs_to_the_top_frame_at_every_early_yield() {
    let harness = Harness::new(CALL_LOOP_PROGRAM);
    harness.yield_interval.set(1);
    harness.park.store(true, Ordering::Release);
    let mut vm = harness.start("user.main");
    let mut previous_top = String::new();
    let mut return_boundaries = 0usize;
    let mut call_boundaries = 0usize;
    loop {
        match vm.exec().expect("exec") {
            bex_vm::VmExecState::Complete(_) => break,
            bex_vm::VmExecState::EarlyYield => {
                let state = vm.export_thread_state().expect("exports");
                let depth = state.frames.len();
                let top = state.frames.last().expect("a frame");
                let function = vm.snapshot_function_of(top.function).expect("a function");
                let name = function.name.clone();
                let line = bex_vm::snapshot::source_line(function, state.cur_pc);
                if name == "user.main" && previous_top != "user.main" && !previous_top.is_empty() {
                    // Back in `main` after a callee returned: the line is one
                    // of the lines of the loop body that make a call.
                    return_boundaries += 1;
                    assert!(
                        (10..=14).contains(&line),
                        "after a return into main the line is {line} (pc {})",
                        state.cur_pc
                    );
                }
                if name == "user.tiny" && previous_top != "user.tiny" {
                    // Just entered `tiny`: nothing of it has run yet.
                    call_boundaries += 1;
                    assert_eq!(state.cur_pc, top.instruction_ptr, "depth {depth}");
                    assert!((2..=3).contains(&line), "entering tiny at line {line}");
                }
                previous_top = name;
            }
            other => panic!("unexpected yield: {other:?}"),
        }
    }
    assert!(return_boundaries >= 24, "{return_boundaries}");
    assert!(call_boundaries >= 24, "{call_boundaries}");
}

/// The loader writes raw heap slots that no TLAB owns. A collection right after
/// the restore has to trace them from the imported VM state, move them, and
/// leave a VM that still runs to the same result. The sys-op arguments held
/// outside the VM are forwarded like the engine forwards its own roots.
#[test]
fn restored_objects_survive_a_collection_in_the_target_heap() {
    for (source, level, early_hops) in [
        (ORDER_PROGRAM, bex_heap::CollectionLevel::Major, 0),
        (ORDER_PROGRAM, bex_heap::CollectionLevel::Minor, 0),
        (CLOSURE_PROGRAM, bex_heap::CollectionLevel::Major, 0),
        (METHOD_PROGRAM, bex_heap::CollectionLevel::Minor, 0),
        (SPIN_PROGRAM, bex_heap::CollectionLevel::Major, 6),
        (SPIN_PROGRAM, bex_heap::CollectionLevel::Minor, 6),
    ] {
        let harness = Harness::new(source);
        let expected = harness.run_plain("user.main");
        harness.gc_after_restore.set(Some(level));
        harness.park.store(early_hops > 0, Ordering::Release);
        let (actual, hops) = harness.run_hopping("user.main", true, early_hops);
        assert_eq!(actual, expected, "collection level {level:?}");
        assert!(hops > 0);
    }
}

#[test]
fn restoring_the_same_snapshot_twice_gives_two_independent_runs() {
    let harness = Harness::new(CLOSURE_PROGRAM);
    let expected = harness.run_plain("user.main");

    // Park at the second `println`, snapshot once, fork twice.
    let (vm, op_name, args) = harness.run_to_sysop("user.main", 1);
    let parked = bex_snapshot::ParkedAt {
        kind: "sysop".to_string(),
        payload: op_name.into_bytes(),
    };
    let (bytes, stats) = harness.write(&vm, parked, args, true, 0).expect("snapshot");
    assert!(stats.objects > 0);
    assert!(stats.compressed_bytes > 0);
    drop(vm);

    for _ in 0..2 {
        let (mut fork, _thread) = harness.restore(&bytes);
        fork.stack.push(bex_vm_types::Value::NULL);
        let result = loop {
            match fork.exec().expect("exec") {
                bex_vm::VmExecState::Complete(value) => break common::render(&fork, value),
                bex_vm::VmExecState::SysOp { .. } => fork.stack.push(bex_vm_types::Value::NULL),
                bex_vm::VmExecState::EarlyYield => {}
                other => panic!("unexpected yield: {other:?}"),
            }
        };
        assert_eq!(result, expected);
    }
}

/// The demo program of the design document: `println` and `sleep` in a
/// `while` loop. The `sleep` yield holds a `baml.time.Duration` argument
/// outside the VM, which must be serializable for the demo to pause there.
const TRIP_PROGRAM: &str = r#"
class TripPlan {
    city string
    ideas string[]
    weather string
}

function remote_fetch_weather(city: string) -> string {
    baml.io.println("[cloud] looking up weather for " + city);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(3000n));
    "sunny in " + city
}

function durable_plan_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function main() -> TripPlan {
    durable_plan_trip("Lisbon")
}
"#;

#[test]
fn the_demo_trip_program_pauses_at_every_println_and_sleep() {
    let harness = Harness::new(TRIP_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(
        expected,
        "TripPlan {city: \"Lisbon\", ideas: [\"day 1 in Lisbon\", \"day 2 in Lisbon\", \
         \"day 3 in Lisbon\"], weather: \"sunny in Lisbon\"}"
    );
    let (actual, hops) = harness.run_hopping("user.main", true, 0);
    assert_eq!(actual, expected);
    // Three loop rounds of println + sleep, then println + sleep in the callee.
    assert_eq!(hops, 8);
}

/// Methods, a bound method value, a generic class (frames and instances carry
/// type arguments whose heads the loader has to rebind), a user-defined type
/// inside the generic, a bigint, and recursion.
const METHOD_PROGRAM: &str = r#"
class Tag {
    name string
}

class Box<T> {
    value T
    function unwrap(self) -> T {
        baml.io.println("unwrap");
        self.value
    }
}

class Counter {
    count int
    function bump(self, by: int) -> int {
        baml.io.println("bump by " + by.to_string());
        self.count += by;
        self.count
    }
}

function depth_sum(n: int) -> int {
    baml.io.println("depth " + n.to_string());
    if (n == 0) { 0 } else { n + depth_sum(n - 1) }
}

class Out {
    counts int[]
    tag Tag
    big bigint
    total int
}

function main() -> Out {
    let counter = Counter { count: 1 };
    let bump = counter.bump;
    let counts: int[] = [];
    counts.push(bump(2));
    counts.push(counter.bump(3));
    let boxed = Box { value: Tag { name: "boxed" } };
    let get = boxed.unwrap;
    let tag = get();
    let big = 123456789012345678901234567890n;
    let total = depth_sum(3);
    Out { counts: counts, tag: tag, big: big + 1n, total: total }
}
"#;

#[test]
fn methods_generics_and_recursion_survive_hops() {
    let harness = Harness::new(METHOD_PROGRAM);
    let expected = harness.run_plain("user.main");
    assert_eq!(
        expected,
        "Out {counts: [3, 6], tag: Tag {name: \"boxed\"}, \
         big: 123456789012345678901234567891n, total: 6}"
    );
    let (actual, hops) = harness.run_hopping("user.main", true, 0);
    assert_eq!(actual, expected);
    // bump x2, unwrap, depth_sum x4.
    assert_eq!(hops, 7);

    // The same at opt level zero, where every local keeps its own slot.
    let unoptimized = Harness::new_unoptimized(METHOD_PROGRAM);
    let (actual, hops) = unoptimized.run_hopping("user.main", false, 0);
    assert_eq!(actual, expected);
    assert_eq!(hops, 7);
}
