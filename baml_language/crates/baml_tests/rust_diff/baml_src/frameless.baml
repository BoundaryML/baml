// Frameless calls: compiled bodies that link no frame record. A body that
// cannot pause runs on its caller's record; one that can (a loop, await,
// interface call or recursion) runs on a pending record linked only when
// something could see the chain. Errors, contexts, depth, spills,
// collections, cancellation, dispatch and telemetry must look as they do with
// a record per call. Under BAML_GC_STRESS=1 every poll yields and collects.

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

interface FxCounter {
    base: int
    function step(self) -> int throws string
    function total(self) -> int throws string {
        self.base + self.step()
    }
}

class FxBad {
    base: int,
    implements FxCounter {
        function step(self) -> int throws string {
            if (self.base < 0) {
                throw `bad:${self.base}`;
            }
            self.base
        }
    }
}

function FxRun(c: FxCounter) -> int {
    c.total()
}

// A throw three frameless levels below its handler, through a default body
// entered with `Self` and an interface arm.
function FxThrows() -> string {
    let r = FxRun(FxBad { base: -2 }) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    `${r}`
}

function FxAwaits(n: int) -> int {
    let total = 0;
    for (let i = 0; i < n; i += 1) {
        total += await spawn { i % 3 };
    }
    total
}

function FxLogs(n: int) -> int {
    log.info("frameless log");
    n + 1
}

function FxAwaitThrows(n: int) -> int {
    let v = await spawn { n };
    if (v == 4) {
        throw `after-await:${v}`;
    }
    v
}

function FxDeep(n: int) -> int {
    FxAwaitThrows(n) + 1
}

// Sibling pending bodies with an interface call at the same pc, on a receiver
// the VM's cached fallback dispatches: each must key the site cache on itself,
// not on the caller they share as the nearest linked record.
interface FxPair {
    function first(self) -> int throws never
    function second(self) -> int throws never
}

// Extra optional parameters leave both methods to the VM.
class FxPadded {
    n: int,
    implements FxPair {
        function first(self, pad: int = 0) -> int throws never { self.n + pad }
        function second(self, pad: int = 0) -> int throws never { self.n * 100 + pad }
    }
}

function FxFirst(p: FxPair) -> int { p.first() }

function FxSecond(p: FxPair) -> int { p.second() }

class FrHeld {
    s: string,
    n: int,
}

// A heap value held across both recursive calls at every level, and a
// looping leaf that polls every 16 iterations, deep inside the recursion.
function FrSpin(k: int) -> int {
    let acc = 0;
    for (let i = 0; i < k; i += 1) {
        acc += i % 3;
    }
    acc
}

function FrFib(n: int) -> int {
    let held = FrHeld { s: `v${n}`, n: n };
    if (n <= 1) {
        return n + FrSpin(40) - FrSpin(40);
    }
    let r = FrFib(n - 1) + FrFib(n - 2);
    if (held.s != `v${n}` || held.n != n) {
        throw `moved:${n}`;
    }
    r
}

// Mutual recursion building a list.
function FrEven(n: int, acc: string[]) -> string[] {
    if (n == 0) {
        return acc;
    }
    acc.push(`e${n}`);
    FrOdd(n - 1, acc)
}

function FrOdd(n: int, acc: string[]) -> string[] {
    if (n == 0) {
        return acc;
    }
    acc.push(`o${n}`);
    FrEven(n - 1, acc)
}

// A body that cannot poll, run on its caller's pending record, throws.
function FrBoom(d: int, s: string) -> int {
    if (s.length() > 0) {
        throw `boom:${d}:${s}`;
    }
    d
}

// A throw from depth 50 through frameless recursion, caught at depth 10 by a
// body that keeps a record.
function FrDescend(d: int, held: FrHeld) -> int {
    if (d == 50) {
        throw `deep:${d}:${held.s}`;
    }
    if (d == 45 && held.n < 0) {
        return FrBoom(d, held.s);
    }
    FrDescend(d + 1, FrHeld { s: `${held.s}.`, n: if (held.n < 0) { -1 } else { d } }) + 1
}

function FrCatchAt(d: int, boom: bool) -> string {
    if (d < 10) {
        return FrCatchAt(d + 1, boom);
    }
    let n = if (boom) { -1 } else { d };
    let r = FrDescend(d, FrHeld { s: "h", n: n }) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    `${r}`
}

// The engine delivers a cancellation at a real suspension, not at a poll, so
// the timeout lands in a sleep at the bottom of a recursion (on the record
// path), each level of which has run frameless recursion that yields under
// GC stress: the records those yields linked are popped, and the chain the
// cancellation unwinds is the record path's.
function FrSlow(n: int) -> int {
    if (n <= 1) {
        return n;
    }
    FrSlow(n - 1) + FrSlow(n - 2)
}

function FrSleepy(n: int) -> int {
    if (n == 0) {
        let warm = FrSlow(12);
        baml.sys.sleep(baml.time.Duration.from_seconds(60));
        return warm;
    }
    FrSlow(6) + FrSleepy(n - 1)
}

function FrSleepyBody() -> string throws never {
    `${FrSleepy(30)}`
}

function FrTimed(body: () -> string throws unknown) -> string {
    baml.future.with_timeout(baml.time.Duration.from_milliseconds(25), body)
    catch_all (e) { _ => e.to_string() }
}

function Frameless() -> void {
    // Sibling pending calls straight from the probe's entry: see FxPair.
    let pair: FxPair = FxPadded { n: 2 };
    baml.io.println(`${FxFirst(pair)} ${FxSecond(pair)}`);

    baml.io.println(FlCatch(0, false));
    baml.io.println(FlCatch(3, false));
    baml.io.println(FlCause());
    baml.io.println(FxThrows());

    // The recursion limit through records with frameless calls at each level
    // is a catchable `StackOverflow`. The depth it fires at depends on native
    // frame sizes, so only its bound is printed.
    let d = Depths { deepest: 0 };
    let a = FlDescend(d, 0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    baml.io.println(`${a}:${d.deepest > 1000}`);

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

    // Calls telemetry records at its default level, through frameless bodies
    // that call a body reading the current span.
    let visible = 0;
    for (let i = 0; i < 5; i += 1) {
        visible += FlVisible(i);
    }
    baml.io.println(`${visible}`);

    baml.io.println(`${FxAwaits(9)}:${FxLogs(2)}`);
    let r = FxDeep(4) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}`);

    baml.io.println(`${FrFib(18)}`);
    let list = FrEven(31, []);
    baml.io.println(`${list.length()}:${list[0]}:${list[1]}:${list[30]}`);
    baml.io.println(FrCatchAt(0, false));
    baml.io.println(FrCatchAt(0, true));
    let timed = FrTimed(FrSleepyBody);
    baml.io.println(`${timed.starts_with("Timeout")}`);

    // A panic from depth 4 that nothing catches: the uncaught trace. Last.
    baml.io.println(FlCatch(3, true));
}
