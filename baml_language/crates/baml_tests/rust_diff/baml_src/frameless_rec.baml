// Frameless recursion: recursive compiled bodies whose only suspension is
// the yield poll run without linking a record per call, and a poll that
// yields first links the records the chain is missing. Spills, collections,
// errors, contexts, depth and cancellation must look as they do with a
// record per call. Under BAML_GC_STRESS=1 every poll yields and collects.

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

// A throw from depth 50 through frameless recursion, caught at depth 10 by a
// body that keeps a record.
// A body that cannot poll, run on its caller's pending record, throws.
function FrBoom(d: int, s: string) -> int {
    if (s.length() > 0) {
        throw `boom:${d}:${s}`;
    }
    d
}

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

// Frameless recursion to the depth limit.
function FrOverflow(d: Depths, n: int) -> int {
    d.deepest = n;
    FrOverflow(d, n + 1) + 1
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

function FramelessRec() -> void {
    baml.io.println(`${FrFib(18)}`);

    let list = FrEven(31, []);
    baml.io.println(`${list.length()}:${list[0]}:${list[1]}:${list[30]}`);

    baml.io.println(FrCatchAt(0, false));
    baml.io.println(FrCatchAt(0, true));

    let d = Depths { deepest: 0 };
    let r = FrOverflow(d, 0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    baml.io.println(`${r}:${d.deepest > 1000}`);

    let timed = FrTimed(FrSleepyBody);
    baml.io.println(`${timed.starts_with("Timeout")}`);
}
