// A loop's post-loop flow is the merge of its exit edges: the condition's
// false edge, which leaves the flow with the loop's own writes dropped, and
// every `break`, each with the flow where it is taken. A C-style step slot
// is entered from the end of the body and from every `continue`.
//
// `loop_narrowing_does_not_escape` pins the zero-iteration half (B-735);
// these pin the break, continue and step edges, a `for let`'s `continue`
// exits and a `break` in a loop header.

class Bag {
    items: int[],
}

function while_break_edge_joins(a: int, s: string) -> int | string {
    let x: int | string = s;
    while ({ x = a; a > 0 }) {
        x = s;
        break;
    }
    x
//  ^ int | string
}

function while_false_edge_keeps_the_condition_write(a: int, s: string) -> int {
    let x: int | string = s;
    while ({ x = a; a > 0 }) {
        x = s;
    }
    x
//  ^ int
}

function while_is_test_exits_with_the_residual(a: int, s: string) -> string {
    let x: int | string = a;
    while (x is int) {
        x = s;
        break;
    }
    x
//  ^ string
}

function break_is_the_only_exit(a: int) -> int {
    let x: int? = null;
    while (true) {
        x = a;
        break;
    }
    x
//  ^ int
}

function defer_runs_on_the_break_edge(a: int, s: string) -> int | string {
    let x: int | string = a;
    while (true) {
        defer {
            x = s;
        }
        x = a;
        break;
    }
    x
//  ^ int | string
}

function inner_break_binds_to_the_inner_loop(a: int, s: string) -> int {
    let x: int | string = s;
    while ({ x = a; a > 0 }) {
        while (true) {
            x = s;
            break;
        }
    }
    x
//  ^ int
}

function condition_write_is_dropped_before_the_condition(a: int, s: string) -> string {
    let x: int | string = a;
    x = a;
    while ({
        let seen = x;
        //  ^^^^ int | string
        x = s;
        a > 0
    }) {}
    x
//  ^ string
}

function step_slot_merges_body_end_and_continue(a: int, s: string, n: int) -> int | string {
    let x: int | string = a;
    let i = 0;
    for (let j = 0; i < n; x = x) {
    //                         ^ int | string
        i += 1;
        if (i == 1) {
            x = s;
            continue;
        }
        x = a;
    }
    x
//  ^ int | string
}

function for_in_collection_runs_once(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = s;
    for let i in { x = a; xs } {
        x = s;
    }
    x
//  ^ int | string
}

function for_in_last_iteration_edge(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = s;
    x = a;
    for let i in xs {
        x = s;
    }
    x
//  ^ int | string
}

function for_in_collection_is_typed_in_the_entry_flow(bag: Bag?) -> int {
    if (bag == null) {
        return 0;
    }
    let total = 0;
    for let v in bag.items {
    //           ^^^^^^^^^ int[]
        total += v;
        bag = null;
    }
    total
}

function while_let_miss_edge_keeps_the_residual(a: int, s: string) -> string {
    let x: int | string = a;
    while let v: int = x {
        x = s;
    }
    x
//  ^ string
}

function for_in_continue_is_an_exit_edge(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = a;
    x = a;
    for let v in xs {
        if (v > 0) {
            x = s;
            continue;
        }
        x = a;
    }
    x
//  ^ int | string
}

function header_break_leaves_the_loop_alive(a: int, s: string) -> int | string {
    let x: int | string = a;
    x = a;
    if (a > 0) {
        x = s;
        while ({ break; }) {}
    }
    x
//  ^ int | string
}

function while_let_header_break_leaves_the_loop_alive(a: int, s: string) -> int | string {
    let x: int | string = a;
    x = a;
    if (a > 0) {
        x = s;
        while let v: int = { break; } {}
    }
    x
//  ^ int | string
}

// The defers armed inside the loop run on a `continue` edge too.
function defer_runs_on_the_continue_edge(a: int, s: string, xs: int[]) -> int | string {
    let x: int | string = a;
    x = a;
    for let v in xs {
        defer {
            x = s;
        }
        x = a;
        continue;
    }
    x
//  ^ int | string
}

// A `continue` in a loop nested inside the update slot binds to that loop.
function nested_loop_in_the_update_slot_may_continue(a: int) -> int {
    let i = 0;
    for (let j = 0; i < 3; i = { for let k in [1] { continue; } i + 1 }) {}
    i
//  ^ int
}

// A pattern refutable by structure alone leaves no residual on the miss edge.
function while_let_non_consuming_miss_edge_keeps_the_type(a: int, b: Bag) -> int | Bag {
    let x: int | Bag = a;
    x = b;
    while let Bag { items: [] } = x {
        x = a;
        break;
    }
    x
//  ^ int | user.Bag
}
