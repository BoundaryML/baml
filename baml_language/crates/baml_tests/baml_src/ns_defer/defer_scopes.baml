// BEP-042 `defer` across scopes: loops, closures, captured cells, scoped
// type bindings, shadowing, and defers that throw on a non-unwinding exit.
//
// A defer body runs exactly once per exit of its block, and every copy of it
// unwinds to the handler in force where the defer was armed — the pad of the
// defer armed before it, or the enclosing `catch` — never to its own landing
// pad. Code after the block, and code before the defer, are outside its pad.
// Names inside the body resolve to the bindings and type parameters visible
// at the defer statement, whatever is in scope at the exit that runs it.
//
// Functions use per-test prefixes so they coexist in one corpus namespace.

class Log { entries: string[] }

// ─── Loops ───────────────────────────────────────────────────────────────────

function loop_cstyle_each_iteration() -> string[] {
    let log: string[] = []
    for (let i = 0; i < 3; i += 1) {
        defer { log.push(`d${i}`) }
    }
    log
}
test "defer_in_cstyle_for_runs_each_iteration" {
    assert.equal(loop_cstyle_each_iteration(), ["d0", "d1", "d2"])
}

function loop_cstyle_continue() -> string[] {
    let log: string[] = []
    for (let i = 0; i < 3; i += 1) {
        defer { log.push(`d${i}`) }
        if (i == 1) { continue }
        log.push(`b${i}`)
    }
    log
}
test "defer_in_cstyle_for_runs_on_continue" {
    assert.equal(loop_cstyle_continue(), ["b0", "d0", "d1", "b2", "d2"])
}

// A `break` from a block nested in the loop body runs the nested block's
// defer and then the loop body's.
function loop_break_from_nested_block() -> string[] {
    let log: string[] = []
    let i = 0
    while (true) {
        defer { log.push("loop") }
        {
            defer { log.push("inner") }
            i += 1
            if (i == 2) { break }
            log.push("body")
        }
    }
    log
}
test "break_from_a_nested_block_runs_both_defers" {
    assert.equal(loop_break_from_nested_block(), ["body", "inner", "loop", "inner", "loop"])
}

// An inner `break` runs only the inner loop's defers.
function loop_nested_inner_break() -> string[] {
    let log: string[] = []
    for (let i in [1, 2]) {
        defer { log.push(`o${i}`) }
        for (let j in [1, 2]) {
            defer { log.push(`i${i}${j}`) }
            if (j == 1) { break }
            log.push("never")
        }
        log.push(`after${i}`)
    }
    log
}
test "inner_break_runs_only_the_inner_loops_defers" {
    assert.equal(loop_nested_inner_break(), ["i11", "after1", "o1", "i21", "after2", "o2"])
}

// A closure created by the defer captures that iteration's cell.
function loop_defer_closure_per_iteration() -> int[] {
    let fs: (() -> int throws never)[] = []
    for (let i in [1, 2, 3]) {
        let j = i * 10
        defer { fs.push(() -> int { j }) }
    }
    let out: int[] = []
    for (let f in fs) { out.push(f()) }
    out
}
test "defer_closure_captures_the_iterations_cell" {
    assert.equal(loop_defer_closure_per_iteration(), [10, 20, 30])
}

// The defer writes the iteration's own cell, which the closures created
// earlier in that iteration read.
function loop_defer_writes_iteration_cell() -> int[] {
    let fs: (() -> int throws never)[] = []
    for (let i in [1, 2, 3]) {
        fs.push(() -> int { i })
        defer { i = i + 100 }
    }
    let out: int[] = []
    for (let f in fs) { out.push(f()) }
    out
}
test "defer_writes_the_iterations_own_cell" {
    assert.equal(loop_defer_writes_iteration_cell(), [101, 102, 103])
}

// In a C-style `for` the defer runs before the step, so a write to `i` in
// the defer is seen by the step (the JS/Go per-iteration binding).
function loop_cstyle_defer_before_step() -> string[] {
    let log: string[] = []
    for (let i = 0; i < 6; i += 1) {
        log.push(`${i}`)
        defer { i = i + 1 }
    }
    log
}
test "cstyle_for_runs_the_defer_before_the_step" {
    assert.equal(loop_cstyle_defer_before_step(), ["0", "2", "4"])
}

function loop_throw_mid_loop_body(log: string[]) -> void {
    for (let i in [1, 2, 3]) {
        defer { log.push(`d${i}`) }
        if (i == 2) { throw "x" }
    }
}
function loop_throw_mid_loop() -> string[] {
    let log: string[] = []
    loop_throw_mid_loop_body(log) catch (e) { _ => { log.push("caught") } }
    log
}
test "a_throw_mid_loop_runs_that_iterations_defer" {
    assert.equal(loop_throw_mid_loop(), ["d1", "d2", "caught"])
}

// A `break` replays a defer body that declares a captured `let`.
function loop_break_replays_captured_let() -> int[] {
    let fs: (() -> int throws never)[] = []
    for (let i in [1, 2, 3]) {
        defer { let k = i * 2; fs.push(() -> int { k }) }
        if (i == 2) { break }
    }
    let out: int[] = []
    for (let f in fs) { out.push(f()) }
    out
}
test "break_replays_a_defer_that_declares_a_captured_let" {
    assert.equal(loop_break_replays_captured_let(), [2, 4])
}

// ─── Closures ────────────────────────────────────────────────────────────────

function closure_defer_in_lambda() -> string[] {
    let log: string[] = []
    let f = () -> int { defer { log.push("lam") }; return 1 }
    let v = f()
    log.push(`${v}`)
    log
}
test "defer_in_a_lambda_runs_on_its_return" {
    assert.equal(closure_defer_in_lambda(), ["lam", "1"])
}

// A lambda is its own cleanup region: calling it never replays the
// enclosing block's defers.
function closure_does_not_replay_parent_defers() -> string[] {
    let log: string[] = []
    {
        defer { log.push("outer") }
        let g = () -> void { log.push("g") }
        g()
        g()
    }
    log
}
test "a_lambda_does_not_replay_the_parents_defers" {
    assert.equal(closure_does_not_replay_parent_defers(), ["g", "g", "outer"])
}

// A closure created by the defer body, on each kind of exit.
function closure_from_defer_normal_exit() -> int {
    let fs: (() -> int throws never)[] = []
    {
        defer { let k = 7; fs.push(() -> int { k }) }
    }
    fs[0]()
}
test "defer_body_closure_on_normal_exit" {
    assert.equal(closure_from_defer_normal_exit(), 7)
}

function closure_from_defer_boom() -> void { throw "x" }
function closure_from_defer_unwind_body(fs: (() -> int throws never)[]) -> void {
    defer { let k = 7; fs.push(() -> int { k }) }
    closure_from_defer_boom()
}
function closure_from_defer_unwind_exit() -> int {
    let fs: (() -> int throws never)[] = []
    closure_from_defer_unwind_body(fs) catch (e) { _ => {} }
    fs[0]()
}
test "defer_body_closure_on_unwind_exit" {
    assert.equal(closure_from_defer_unwind_exit(), 7)
}

function closure_from_defer_return_body(fs: (() -> int throws never)[], early: bool) -> int {
    defer { let k = 7; fs.push(() -> int { k }) }
    if (early) { return 1 }
    2
}
function closure_from_defer_return_exit() -> int {
    let fs: (() -> int throws never)[] = []
    let _ = closure_from_defer_return_body(fs, true)
    fs[0]()
}
test "defer_body_closure_on_return_exit" {
    assert.equal(closure_from_defer_return_exit(), 7)
}

function closure_throwing_lambda_defer() -> string[] {
    let log: string[] = []
    let f = () -> void { defer { log.push("lamd") }; throw "x" }
    f() catch (e) { _ => { log.push("c") } }
    log
}
test "defer_in_a_throwing_lambda_runs_before_the_callers_catch" {
    assert.equal(closure_throwing_lambda_defer(), ["lamd", "c"])
}

function closure_defer_in_spawn_body() -> string[] {
    let log: string[] = []
    let fut = spawn { defer { log.push("sd") }; 1 }
    let v = await fut
    log.push(`${v}`)
    log
}
test "defer_in_a_spawn_body_runs_before_the_future_settles" {
    assert.equal(closure_defer_in_spawn_body(), ["sd", "1"])
}

function closure_from_defer_sees_later_write() -> int {
    let fs: (() -> int throws never)[] = []
    let x = 5
    {
        defer { fs.push(() -> int { x }) }
    }
    x = 6
    fs[0]()
}
test "a_closure_created_by_a_defer_sees_a_later_write" {
    assert.equal(closure_from_defer_sees_later_write(), 6)
}

// ─── Captured cells ──────────────────────────────────────────────────────────

function cell_defer_reads_final_value() -> string[] {
    let log: string[] = []
    let x = 1
    let g = () -> void { x = 5 }
    {
        defer { log.push(`${x}`) }
        g()
    }
    log
}
test "defer_reads_a_captured_locals_final_value" {
    assert.equal(cell_defer_reads_final_value(), ["5"])
}

function cell_defer_writes_captured() -> int {
    let x = 1
    let g = () -> int { x }
    {
        defer { x = 9 }
    }
    g()
}
test "defer_writes_a_captured_local_through_its_cell" {
    assert.equal(cell_defer_writes_captured(), 9)
}

function cell_defer_writes_captured_parameter(p: int) -> int {
    let g = () -> int { p }
    {
        defer { p = p + 1 }
    }
    g()
}
test "defer_writes_a_captured_parameter" {
    assert.equal(cell_defer_writes_captured_parameter(1), 2)
}

// A defer inside a lambda, run by that lambda's landing pad, writes a
// variable captured from the enclosing function.
function cell_pad_boom() -> void { throw "x" }
function cell_defer_in_lambda_pad_writes_capture() -> int {
    let x = 1
    let g = () -> int { x }
    let h = () -> void { defer { x = 7 }; cell_pad_boom() }
    h() catch (e) { _ => {} }
    g()
}
test "a_defer_in_a_lambdas_landing_pad_writes_a_capture" {
    assert.equal(cell_defer_in_lambda_pad_writes_capture(), 7)
}

// The return operand (and a block's tail value) is evaluated before the
// defers run: a defer that writes the returned variable does not change
// what is returned.
function cell_return_operand_before_defers() -> int {
    let x = 1
    defer { x = 2 }
    return x
}
test "the_return_operand_is_evaluated_before_the_defers" {
    assert.equal(cell_return_operand_before_defers(), 1)
}

function cell_return_captured_operand_before_defers() -> int {
    let x = 1
    let g = () -> int { x }
    defer { x = 2 }
    return x
}
test "a_captured_return_operand_is_evaluated_before_the_defers" {
    assert.equal(cell_return_captured_operand_before_defers(), 1)
}

function cell_tail_value_before_defers() -> int {
    let x = 1
    let g = () -> int { x }
    defer { x = 2 }
    x
}
test "a_captured_tail_value_is_evaluated_before_the_defers" {
    assert.equal(cell_tail_value_before_defers(), 1)
}

function cell_return_field_before_defers() -> int {
    let o = UnwindCounter { n: 1 }
    defer { o.n = 2 }
    return o.n
}
test "a_returned_field_read_happens_before_the_defer_writes_it" {
    assert.equal(cell_return_field_before_defers(), 1)
}

// ─── Scoped type bindings ────────────────────────────────────────────────────

function scoped_type_normal_exit(t: reflect.Type) -> string[] {
    let log: string[] = []
    {
        type T = unreflect(t)
        defer { log.push(reflect.Type.of<T>().to_string()) }
    }
    log
}
test "a_defer_names_a_scoped_type_on_normal_exit" {
    assert.equal(scoped_type_normal_exit(reflect.Type.of<int>()), ["int"])
}

function scoped_type_per_iteration() -> string[] {
    let log: string[] = []
    for (let t in [reflect.Type.of<int>(), reflect.Type.of<string>()]) {
        type T = unreflect(t)
        defer { log.push(reflect.Type.of<T>().to_string()) }
    }
    log
}
test "a_defer_sees_each_iterations_type_binding" {
    assert.equal(scoped_type_per_iteration(), ["int", "string"])
}

function scoped_type_unwind_body(log: string[], t: reflect.Type) -> void {
    type T = unreflect(t)
    defer { log.push(reflect.Type.of<T>().to_string()) }
    throw "x"
}
function scoped_type_unwind() -> string[] {
    let log: string[] = []
    scoped_type_unwind_body(log, reflect.Type.of<int>()) catch (e) { _ => { log.push("c") } }
    log
}
test "a_defer_names_a_scoped_type_on_unwind" {
    assert.equal(scoped_type_unwind(), ["int", "c"])
}

function scoped_type_nested_return_body(log: Log, t: reflect.Type, early: bool) -> int {
    {
        type T = unreflect(t)
        defer { log.entries.push(reflect.Type.of<T>().to_string()) }
        if (early) { return 1 }
    }
    2
}
function scoped_type_nested_return() -> string[] {
    let log = Log { entries: [] }
    let r = scoped_type_nested_return_body(log, reflect.Type.of<int>(), true)
    log.entries.push(`${r}`)
    log.entries
}
test "a_defer_names_a_scoped_type_on_a_nested_return" {
    assert.equal(scoped_type_nested_return(), ["int", "1"])
}

// The defer body binds a type of its own, on the return and normal exits.
function scoped_type_bound_in_defer_body(log: Log, t: reflect.Type, early: bool) -> int {
    defer {
        type U = unreflect(t)
        log.entries.push(reflect.Type.of<U>().to_string())
    }
    if (early) { return 1 }
    2
}
function scoped_type_bound_in_defer() -> string[] {
    let log = Log { entries: [] }
    let _ = scoped_type_bound_in_defer_body(log, reflect.Type.of<int>(), true)
    let _ = scoped_type_bound_in_defer_body(log, reflect.Type.of<string>(), false)
    log.entries
}
test "a_defer_body_binds_its_own_scoped_type" {
    assert.equal(scoped_type_bound_in_defer(), ["int", "string"])
}

// A defer armed before a `type T = unreflect(e)` whose `e` throws runs.
function scoped_type_source_boom() -> reflect.Type throws string { throw "x" }
function scoped_type_source_throws_body(log: string[]) -> void {
    defer { log.push("d") }
    type T = unreflect(scoped_type_source_boom())
}
function scoped_type_source_throws() -> string[] {
    let log: string[] = []
    scoped_type_source_throws_body(log) catch (e) { _ => { log.push("c") } }
    log
}
test "a_defer_runs_when_a_type_bindings_source_throws" {
    assert.equal(scoped_type_source_throws(), ["d", "c"])
}

// An outer defer naming the function's `T` is replayed at a `return`
// inside a block that binds its own `type T`: the body still names the
// function's parameter, not the shadow in scope at the exit.
function scoped_type_shadow_return_body<T>(log: Log, t: reflect.Type, early: bool) -> int {
    defer { log.entries.push(reflect.Type.of<T>().to_string()) }
    {
        type T = unreflect(t)
        log.entries.push(reflect.Type.of<T>().to_string())
        if (early) { return 1 }
    }
    2
}
function scoped_type_shadow_return() -> string[] {
    let log = Log { entries: [] }
    let _ = scoped_type_shadow_return_body<string>(log, reflect.Type.of<int>(), true)
    log.entries
}
test "an_outer_defer_replayed_under_a_shadowing_type_binding_keeps_its_own_T" {
    assert.equal(scoped_type_shadow_return(), ["int", "string"])
}

function scoped_type_shadow_continue_body<T>(log: Log, t: reflect.Type) -> void {
    for (let i in [1, 2]) {
        defer { log.entries.push(reflect.Type.of<T>().to_string()) }
        {
            type T = unreflect(t)
            if (i == 1) { continue }
        }
    }
}
function scoped_type_shadow_continue() -> string[] {
    let log = Log { entries: [] }
    scoped_type_shadow_continue_body<string>(log, reflect.Type.of<int>())
    log.entries
}
test "an_outer_defer_replayed_at_continue_under_a_shadow_keeps_its_own_T" {
    assert.equal(scoped_type_shadow_continue(), ["string", "string"])
}

function scoped_type_shadow_unwind_body<T>(log: Log, t: reflect.Type) -> void {
    defer { log.entries.push(reflect.Type.of<T>().to_string()) }
    {
        type T = unreflect(t)
        throw "x"
    }
}
function scoped_type_shadow_unwind() -> string[] {
    let log = Log { entries: [] }
    scoped_type_shadow_unwind_body<string>(log, reflect.Type.of<int>()) catch (e) { _ => { log.entries.push("c") } }
    log.entries
}
test "an_outer_defers_pad_under_a_shadow_keeps_its_own_T" {
    assert.equal(scoped_type_shadow_unwind(), ["string", "c"])
}

// ─── Shadowing ───────────────────────────────────────────────────────────────

function shadow_defer_binds_at_its_site() -> string[] {
    let log: string[] = []
    let x = "outer"
    {
        defer { log.push(x) }
        let x = "inner"
        log.push(x)
    }
    log
}
test "a_defer_names_the_binding_visible_at_its_site" {
    assert.equal(shadow_defer_binds_at_its_site(), ["inner", "outer"])
}

function shadow_defer_writes_outer() -> string[] {
    let log: string[] = []
    let x = 1
    {
        defer { x = 2 }
        let x = 3
        log.push(`${x}`)
    }
    log.push(`${x}`)
    log
}
test "a_defer_writes_the_outer_binding_past_a_shadow" {
    assert.equal(shadow_defer_writes_outer(), ["3", "2"])
}

function shadow_inside_defer_body() -> string[] {
    let log: string[] = []
    let x = 1
    {
        defer { let x = 2; log.push(`${x}`) }
    }
    log.push(`${x}`)
    log
}
test "a_shadow_declared_in_a_defer_body_is_local_to_it" {
    assert.equal(shadow_inside_defer_body(), ["2", "1"])
}

function shadow_match_arm_binding() -> string {
    let fs: (() -> string throws never)[] = []
    let v: int | string = "s"
    match (v) {
        let s: string => { defer { fs.push(() -> string { s }) } },
        _ => {},
    }
    fs[0]()
}
test "a_match_arm_binding_is_captured_from_a_defer" {
    assert.equal(shadow_match_arm_binding(), "s")
}

function shadow_catch_arm_boom() -> void throws string { throw "x" }
function shadow_catch_arm_binding() -> string[] {
    let log: string[] = []
    shadow_catch_arm_boom() catch (e) {
        let e: string => { defer { log.push(e) }; log.push("arm") },
    }
    log
}
test "a_catch_arm_binding_is_read_by_a_defer" {
    assert.equal(shadow_catch_arm_binding(), ["arm", "x"])
}

// ─── Defers that throw on a non-unwinding exit ───────────────────────────────
//
// The inline copy of a defer body runs outside its own landing pad, so a
// throw or panic from it runs the body once — never a second time through
// the pad — and still runs the defers armed before it.

function throwing_normal_exit_body(log: string[]) -> void {
    defer { log.push("b"); throw "x" }
}
function throwing_normal_exit() -> string[] {
    let log: string[] = []
    throwing_normal_exit_body(log) catch (e) { _ => { log.push(`c:${e}`) } }
    log
}
test "a_throwing_defer_on_normal_exit_runs_once" {
    assert.equal(throwing_normal_exit(), ["b", "c:x"])
}

function throwing_earlier_defer_still_runs_body(log: string[]) -> void {
    defer { log.push("a") }
    defer { log.push("b"); throw "x" }
}
function throwing_earlier_defer_still_runs() -> string[] {
    let log: string[] = []
    throwing_earlier_defer_still_runs_body(log) catch (e) { _ => { log.push(`c:${e}`) } }
    log
}
test "a_throwing_defer_on_normal_exit_still_runs_the_earlier_defer_once" {
    assert.equal(throwing_earlier_defer_still_runs(), ["b", "a", "c:x"])
}

function throwing_on_return_body(log: string[]) -> int {
    defer { log.push("b"); throw "x" }
    return 1
}
function throwing_on_return() -> string[] {
    let log: string[] = []
    let _ = throwing_on_return_body(log) catch (e) { _ => { log.push(`c:${e}`); 0 } }
    log
}
test "a_throwing_defer_on_return_runs_once" {
    assert.equal(throwing_on_return(), ["b", "c:x"])
}

function throwing_at_iteration_end_body(log: string[]) -> void {
    for (let i in [1, 2]) {
        defer { log.push(`d${i}`); throw "x" }
    }
}
function throwing_at_iteration_end() -> string[] {
    let log: string[] = []
    throwing_at_iteration_end_body(log) catch (e) { _ => { log.push(`c:${e}`) } }
    log
}
test "a_throwing_defer_at_iteration_end_runs_once" {
    assert.equal(throwing_at_iteration_end(), ["d1", "c:x"])
}

function throwing_panic_on_normal_exit_body(log: string[]) -> void {
    defer { log.push("p"); let z = 10 / 0 }
}
function throwing_panic_on_normal_exit() -> string[] {
    let log: string[] = []
    throwing_panic_on_normal_exit_body(log) catch (e) { baml.panics.DivisionByZero => { log.push("c") } }
    log
}
test "a_panicking_defer_on_normal_exit_runs_once" {
    assert.equal(throwing_panic_on_normal_exit(), ["p", "c"])
}

// Two throwing defers on the normal exit: the later-armed one throws first
// (LIFO), the earlier one still runs and its throw propagates with the first
// as its cause.
function throwing_two_defers_body(log: string[]) -> void {
    defer { log.push("a"); throw "A" }
    defer { log.push("b"); throw "B" }
}
function throwing_two_defers() -> string[] {
    let log: string[] = []
    throwing_two_defers_body(log) catch (e, ctx) {
        _ => {
            log.push(`c:${e}`)
            match (ctx.cause) {
                let c: baml.errors.Context => { log.push(`cause:${c.error}`) },
                _ => { log.push("nocause") },
            }
        }
    }
    log
}
test "two_throwing_defers_on_normal_exit_each_run_once_and_chain" {
    assert.equal(throwing_two_defers(), ["b", "a", "c:A", "cause:B"])
}

// A defer whose body completes while an error unwinds re-raises that error to
// the next handler out, and landing there fills the handler's error AND
// context. Two shapes where the next handler is in the same function, which
// once got the error alone:

// ...a `catch (e, ctx)` right outside the block: `ctx` must be the context of
// the error it caught, not an unwritten slot (reading one was a fatal VM
// internal error, not a catchable one).
function cascade_boom() -> void { throw "x" }
function same_function_catch_sees_its_context() -> string[] {
    let log: string[] = []
    let _ = {
        defer { log.push("d") }
        cascade_boom()
        1
    } catch (e, ctx) {
        _ => { log.push(`c:${e}`); log.push(`ctx:${ctx.error}`); 0 }
    }
    log
}
test "a defer cascading into a catch in the same function hands it the context" {
    assert.equal(same_function_catch_sees_its_context(), ["d", "c:x", "ctx:x"])
}

// ...an outer block's defer: the inner pad cascades across blocks into the
// outer pad, whose throw must still chain the original error as its cause.
// (Two defers in ONE block share a pad's slots and always kept the chain.)
function nested_defers_body(log: string[]) -> void {
    defer { log.push("A"); throw "A" }
    {
        defer { log.push("B") }
        cascade_boom()
    }
}
function nested_defers_keep_the_cause() -> string[] {
    let log: string[] = []
    nested_defers_body(log) catch (e, ctx) {
        _ => {
            log.push(`e:${e}`)
            match (ctx.cause) {
                let c: baml.errors.Context => { log.push(`cause:${c.error}`) },
                _ => { log.push("nocause") },
            }
        }
    }
    log
}
test "a defer cascading into an outer block's defer keeps the cause chain" {
    assert.equal(nested_defers_keep_the_cause(), ["B", "A", "e:A", "cause:x"])
}

function throwing_catch_then_defer_boom() -> void { throw "x" }
function throwing_catch_then_defer() -> string[] {
    let log: string[] = []
    {
        defer { log.push("d") }
        throwing_catch_then_defer_boom() catch (e) { _ => { log.push("c") } }
    }
    log
}
test "a_defer_runs_after_the_catch_handler_in_its_block" {
    assert.equal(throwing_catch_then_defer(), ["c", "d"])
}

function throwing_nested_pads_boom() -> void { throw "x" }
function throwing_nested_pads_body(log: string[]) -> void {
    defer { log.push("A") }
    {
        defer { log.push("B") }
        throwing_nested_pads_boom()
    }
}
function throwing_nested_pads() -> string[] {
    let log: string[] = []
    throwing_nested_pads_body(log) catch (e) { _ => { log.push("c") } }
    log
}
test "nested_blocks_pads_cascade_outward" {
    assert.equal(throwing_nested_pads(), ["B", "A", "c"])
}

function throwing_defer_suspends_boom() -> void { throw "x" }
function throwing_defer_suspends_body(log: string[]) -> void {
    defer {
        let f = spawn { 1 }
        let v = await f
        log.push(`aw${v}`)
    }
    throwing_defer_suspends_boom()
}
function throwing_defer_suspends() -> string[] {
    let log: string[] = []
    throwing_defer_suspends_body(log) catch (e) { _ => { log.push("c") } }
    log
}
test "a_defer_may_suspend_on_the_unwind_path" {
    assert.equal(throwing_defer_suspends(), ["aw1", "c"])
}

// ─── Code outside a block's pads ─────────────────────────────────────────────

// A throw after the block that armed the defer does not run it again.
function after_block_boom() -> void { throw "x" }
function after_block_body(log: string[]) -> void {
    {
        defer { log.push("A") }
    }
    after_block_boom()
}
function after_block() -> string[] {
    let log: string[] = []
    after_block_body(log) catch (e) { _ => { log.push("c") } }
    log
}
test "code_after_a_block_is_outside_its_defers_pad" {
    assert.equal(after_block(), ["A", "c"])
}

function after_catch_boom() -> void { throw "x" }
function after_catch_body(log: string[]) -> void {
    after_catch_boom() catch (e) { _ => { log.push("c1") } }
    after_catch_boom()
}
function after_catch() -> string[] {
    let log: string[] = []
    after_catch_body(log) catch (e) { _ => { log.push("c2") } }
    log
}
test "code_after_a_catch_expression_is_outside_its_handler" {
    assert.equal(after_catch(), ["c1", "c2"])
}

function after_loop_boom() -> void { throw "x" }
function after_loop_body(log: string[]) -> void {
    for (let i in [1, 2]) {
        defer { log.push(`d${i}`) }
    }
    after_loop_boom()
}
function after_loop() -> string[] {
    let log: string[] = []
    after_loop_body(log) catch (e) { _ => { log.push("c") } }
    log
}
test "code_after_a_loop_with_a_defer_is_outside_its_pad" {
    assert.equal(after_loop(), ["d1", "d2", "c"])
}

// A trap before a catch expression, in the same block, is not caught by it.
function before_catch_ok() -> void {}
function before_catch_body(log: string[], d: int) -> int {
    let z = 10 / d
    before_catch_ok() catch (e) { _ => { log.push("inner") } }
    z
}
function before_catch() -> string[] {
    let log: string[] = []
    let _ = before_catch_body(log, 0) catch (e) { baml.panics.DivisionByZero => { log.push("outer"); 0 } }
    log
}
test "a_trap_before_a_catch_expression_is_not_caught_by_it" {
    assert.equal(before_catch(), ["outer"])
}

// ─── Nested handlers ─────────────────────────────────────────────────────────
//
// A block unwinds to exactly one handler, the innermost; an outer handler is
// reached hop by hop — the pad cascades to the catch it was armed under, and
// a catch's arms unwind to the catch outside it.

// Throws an int or a string, so both arms below are reachable.
function nested_throw(an_int: bool) -> void throws int | string {
    if (an_int) { throw 123 } else { throw "s" }
}

function nested_int_body(log: string[]) -> void {
    {
        {
            defer { log.push("left") }
            nested_throw(true)
        } catch (_) {
            int => { log.push("inner:int") }
        }
    } catch (_) {
        string => { log.push("outer:string") }
    }
}
test "an int lands in the inner catch after the defer" {
    let log: string[] = []
    nested_int_body(log)
    assert.equal(log, ["left", "inner:int"])
}

function nested_string_body(log: string[]) -> void {
    {
        {
            defer { log.push("left") }
            nested_throw(false)
        } catch (_) {
            int => { log.push("inner:int") }
        }
    } catch (_) {
        string => { log.push("outer:string") }
    }
}
test "a string passes the inner arms and lands in the outer catch after the defer" {
    let log: string[] = []
    nested_string_body(log)
    assert.equal(log, ["left", "outer:string"])
}

function nested_throwing_defer_body(log: string[]) -> void {
    {
        {
            defer { log.push("left"); throw "from defer" }
            nested_throw(true)
        } catch (_) {
            int => { log.push("inner:int") }
        }
    } catch (e, ctx) {
        string => {
            log.push(`outer:${e}`)
            match (ctx.cause) {
                let c: baml.errors.Context => { log.push(`cause:${c.error}`) },
                _ => { log.push("nocause") },
            }
        }
    }
}
test "a defer that throws while unwinding replaces the error and chains it" {
    let log: string[] = []
    nested_throwing_defer_body(log)
    assert.equal(log, ["left", "outer:from defer", "cause:123"])
}

function nested_normal_exit_body(log: string[]) -> void {
    {
        {
            defer { log.push("left") }
            log.push("body")
        } catch (_) {
            int => { log.push("inner:int") }
        }
        log.push("after inner")
    } catch (_) {
        string => { log.push("outer:string") }
    }
}
test "with no throw the defer runs once and nothing is caught" {
    let log: string[] = []
    nested_normal_exit_body(log)
    assert.equal(log, ["body", "left", "after inner"])
}
