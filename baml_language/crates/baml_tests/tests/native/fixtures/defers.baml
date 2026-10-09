// Native backend fixture: `defer` on normal exit, early return, break,
// continue and unwind, in the VM's order, observed through a log.

class Invalid { message: string }
class Coded { code: int, message: string }

function defer_order(x: int) -> string {
    let log = "";
    {
        defer { log = log + "A;" }
        defer { log = log + "B;" }
        log = log + "body" + x.to_string() + ";";
    }
    log + "after;"
}

function defer_sees_final(x: int) -> string {
    let log = "";
    {
        let status = "pending";
        defer { log = "final:" + status }
        status = "done" + x.to_string();
    }
    log
}

function early_return(log: string[], early: bool) -> int {
    defer { log.push("cleanup") }
    if (early) { return 1 }
    log.push("after");
    2
}

function defer_early_return(early: bool) -> string {
    let log: string[] = [];
    let v = early_return(log, early);
    log.to_string() + v.to_string()
}

function defer_loop(n: int) -> string {
    let log: string[] = [];
    for (let i in [1, 2, 3, 4]) {
        defer { log.push("d" + i.to_string()) }
        if (i == n) { break }
        if (i % 2 == 1) { continue }
        log.push("i" + i.to_string());
    }
    log.to_string()
}

function defer_while(n: int) -> string {
    let log: string[] = [];
    let i = 0;
    while (i < 4) {
        i = i + 1;
        defer { log.push("w" + i.to_string()) }
        if (i == n) { continue }
        log.push("b" + i.to_string());
    }
    log.to_string()
}

function unwinds(log: string[], x: int) -> int {
    defer { log.push("outer") }
    defer { log.push("inner") }
    if (x < 0) { throw Invalid { message: "negative" } }
    let v = 10 / x;
    defer { log.push("late") }
    log.push("body");
    v
}

function defer_unwind(x: int) -> string {
    let log: string[] = [];
    let v = unwinds(log, x) catch (e) {
        Invalid => { log.push("caught invalid"); -1 },
        baml.panics.DivisionByZero => { log.push("caught panic"); -2 },
    };
    log.to_string() + v.to_string()
}

function throwing_defer(log: string[], x: int) -> int {
    defer { log.push("a") }
    defer { log.push("b"); throw Coded { code: x, message: "from_b" } }
    if (x < 0) { throw Invalid { message: "x" } }
    log.push("body");
    x
}

function defer_throws(x: int) -> string {
    let log: string[] = [];
    let v = throwing_defer(log, x).to_string() catch (e) {
        Invalid => "invalid",
        let c: Coded => "coded" + c.code.to_string(),
    };
    log.to_string() + v
}

function defer_throw_uncaught(x: int) -> int {
    defer { if (x < 0) { throw Coded { code: x, message: "deferred" } } }
    if (x == 0) { throw Invalid { message: "body" } }
    x
}

function defer_in_handler(x: int) -> string {
    let log: string[] = [];
    let v = (unwinds(log, x) catch (e) {
        Invalid => {
            defer { log.push("handler cleanup") }
            log.push("handler");
            -1
        }
    });
    log.to_string() + v.to_string()
}

function defer_nested_blocks(x: int) -> string {
    let log: string[] = [];
    {
        defer { log.push("outer") }
        {
            defer { log.push("inner") }
            if (x == 1) { log.push("one") }
        }
        log.push("between");
    }
    log.to_string()
}
