// Frameless calls: compiled bodies that cannot pause run on their caller's
// frame record. Errors, contexts, depth, yields and telemetry must look as
// they do with a record per call.

function FlThrow(n: int) -> int {
    if (n == 3) {
        throw `frameless:${n}`;
    }
    n + 1
}

function FlDivide(n: int) -> int {
    let doubled = 100 / (n - 3);
    doubled + 1
}

function FlInner(n: int, divide: bool) -> int {
    if (divide) {
        return FlDivide(n) * 3;
    }
    FlThrow(n) * 3
}

function FlOuter(n: int, divide: bool) -> int {
    let r = FlInner(n, divide);
    r + 2
}

// Depth 1 catches what depth 4 raised through three frameless bodies.
function FlCatch(n: int, divide: bool) -> string {
    let r = FlOuter(n, divide) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    `${r}`
}

function FlCatchPanic(n: int) -> string {
    let r = FlOuter(n, true) catch_all_panics (e, ctx) {
        _ => `${Frames(ctx)}`,
    };
    `${r}`
}

// A throw inside a handler: its cause is the context of the first error.
function FlCause() -> string {
    let r = FlOuter(3, false) catch (e, ctx) {
        _ => {
            FlOuter(3, true) catch_all_panics (inner, context) {
                _ => match (context.root_cause().error) {
                    let s: string => `${s}|${Frames(context)}`,
                    _ => "missing cause",
                },
            }
        },
    };
    `${r}`
}

function FlAdd(a: int, b: int) -> int {
    a + b
}

function FlTwice(a: int) -> int {
    FlAdd(a, a)
}

function FlDescend(d: Depths, n: int) -> int {
    d.deepest = n;
    FlTwice(n) - 2 * n + 1 + FlDescend(d, n + 1)
}

function FlBounce(d: Depths, n: int) -> int {
    d.deepest = n;
    let r = FlDescend(d, n + 1) catch (e) {
        let e: baml.panics.StackOverflow => -n,
    };
    r
}

function FlConcat(s: string, i: int) -> string {
    `${s}${i % 10}`
}

function FlPoint(x: int, y: int) -> int[] {
    [x, y, FlAdd(x, y)]
}

function FlSpan() -> bool {
    trace.current_span_id() != null
}

function FlVisible(n: int) -> int {
    if (FlSpan()) {
        return FlAdd(n, 1);
    }
    FlAdd(n, 1)
}

function Frameless() -> void {
    baml.io.println(FlCatch(0, false));
    baml.io.println(FlCatch(3, false));
    baml.io.println(FlCatchPanic(3));
    baml.io.println(FlCause());

    // The recursion limit through records with frameless calls at each level
    // is a catchable `StackOverflow` wherever the handler is. The depth it
    // fires at depends on native frame sizes, so only its bound is printed;
    // the VM tests compare it with the record path.
    let d = Depths { deepest: 0 };
    let a = FlDescend(d, 0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    let first = d.deepest;
    let b = FlBounce(d, 0);
    baml.io.println(`${a}:${first > 1000}:${b}:${d.deepest > 1000}`);

    // A looping caller polls, yields and collects between frameless calls
    // that allocate.
    let s = "";
    let points: int[][] = [];
    for (let i = 0; i < 3000; i += 1) {
        s = FlConcat(s, i);
        if (i % 100 == 0) {
            points.push(FlPoint(i, FlTwice(i)));
        }
    }
    let total = 0;
    for (let p in points) {
        total += p[2];
    }
    baml.io.println(`${s.length()}:${points.length()}:${total}`);

    // Calls telemetry records at its default level, through frameless
    // bodies that call a body reading the current span.
    let visible = 0;
    for (let i = 0; i < 5; i += 1) {
        visible += FlVisible(i);
    }
    baml.io.println(`${visible}`);

    // A panic from depth 4 that nothing catches: the uncaught trace.
    baml.io.println(FlCatch(3, true));
}
