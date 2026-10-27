// Adversarial probes for frame records: errors, suspension, spawns,
// cancellation and depth accounting below chains of directly called bodies.

function SpawnLeafPanic(xs: int[], n: int) -> int {
    if (n == 0) {
        return xs[10];
    }
    SpawnLeafPanic(xs, n - 1) + 1
}

function SpawnLeafThrow(n: int) -> int {
    if (n == 0) {
        throw `spawned:${n}`;
    }
    SpawnLeafThrow(n - 1) + 1
}

// A task whose body panics or throws below a chain of directly called bodies,
// awaited and handled by the parent.
function SpawnPanicCaught() -> void {
    let p = spawn { SpawnLeafPanic([1], 6) };
    let a = (await p) catch_all_panics (e, ctx) {
        _ => -1,
    };
    let t = spawn { SpawnLeafThrow(6) };
    let b = (await t) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${a}:${b}`);
}

// The same panic, uncaught: the program must fail, not hang.
function SpawnPanicUncaught() -> void {
    let p = spawn { SpawnLeafPanic([1], 6) };
    let v = await p;
    baml.io.println(`unreachable:${v}`);
}

// A thrown error from a task, uncaught.
function SpawnThrowUncaught() -> void {
    let t = spawn { SpawnLeafThrow(6) };
    let v = await t;
    baml.io.println(`unreachable:${v}`);
}

// A panicking task nobody awaits.
function SpawnPanicDetached() -> void {
    let _ = spawn { SpawnLeafPanic([1], 6) };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(20));
    baml.io.println("parent-done");
}

function SpawnInChainLeaf(n: int) -> int {
    let t = spawn { SpawnLeafThrow(n) };
    await t
}

function SpawnInChain(n: int, catch_at: int) -> int {
    if (n == 0) {
        return SpawnInChainLeaf(3);
    }
    if (n == catch_at) {
        let r = SpawnInChain(n - 1, catch_at) catch (e, ctx) {
            _ => -100 - n,
        };
        return r;
    }
    SpawnInChain(n - 1, catch_at) + 1
}

// Tasks spawned and awaited deep in a chain, their errors caught at
// different levels of the parent's chain.
function SpawnChains() -> void {
    baml.io.println(`${SpawnInChain(8, 1)}:${SpawnInChain(8, 5)}:${SpawnInChain(8, 8)}`);
    let r = SpawnInChain(8, 99) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}`);
}

function AwaitThrowLeaf(f: baml.future.Future<int, string>) -> int {
    let v = await f;
    v + 1
}

function AwaitThrowMid(f: baml.future.Future<int, string>, n: int, catch_at: int) -> int {
    if (n == 0) {
        return AwaitThrowLeaf(f);
    }
    if (n == catch_at) {
        let r = AwaitThrowMid(f, n - 1, catch_at) catch (e, ctx) {
            _ => -1000 - n,
        };
        return r;
    }
    AwaitThrowMid(f, n - 1, catch_at) + 1
}

// An error delivered into a body parked below a chain, caught at the leaf's
// parent, in the middle, at the top, and not at all.
function AwaitErrors() -> void {
    let a = AwaitThrowMid(spawn { SpawnLeafThrow(2) }, 10, 1);
    let b = AwaitThrowMid(spawn { SpawnLeafThrow(2) }, 10, 6);
    let c = AwaitThrowMid(spawn { SpawnLeafThrow(2) }, 10, 10);
    let d = AwaitThrowMid(spawn { SpawnLeafThrow(2) }, 10, 99) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    let ok = AwaitThrowMid(spawn { Work(5) }, 10, 99);
    baml.io.println(`${a}:${b}:${c}:${ok}`);
    baml.io.println(`${d}`);
}

function DeferLeaf(state: string[], n: int) -> int {
    defer { state.push(`leaf:${n}`); }
    if (n == 0) {
        throw "defer-leaf";
    }
    DeferLeaf(state, n - 1) + 1
}

function DeferSleepy(state: string[]) -> int {
    defer {
        state.push("sleepy-start");
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5));
        let junk = [Point { x: 1 }, Point { x: 2 }];
        state.push(`sleepy-end:${junk.length()}:${Work(50)}`);
    }
    DeferLeaf(state, 4)
}

function DeferTop(state: string[]) -> int {
    defer { state.push("top"); }
    DeferSleepy(state) + 1
}

// `defer`s run in order while an error unwinds a chain, one of them
// suspending, allocating and calling deep.
function DeferUnwind() -> void {
    let state: string[] = [];
    let r = DeferTop(state) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}`);
    baml.io.println(state.join(","));
}

// The same unwind, uncaught.
function DeferUnwindUncaught() -> void {
    let state: string[] = [];
    defer { baml.io.println(state.join(",")); }
    let _ = DeferTop(state);
}

function Measure() -> int {
    let d = Depths { deepest: 0 };
    let _ = Descend(d, 0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    d.deepest
}

function ThrowingClosureChain(n: int) -> int {
    let f = (k: int) -> int {
        if (k > 2) {
            throw `c:${k}`;
        }
        k
    };
    ApplyDeep(f, n)
}

// Errors caught over and over must leave no frames behind: the depth a
// recursion reaches afterwards, and inside a handler, matches the depth before.
// A leak would cost a frame per caught error, 1200 here. Under compiled bodies
// with VM frames the depth is bounded by the Rust stack, and each call site of
// `Measure` sits in a generated wrapper of its own size, so the depths may
// differ by a frame or two without anything leaking.
function NoLeaks() -> void {
    let before = Measure();
    let i = 0;
    let caught = 0;
    while (i < 300) {
        let a = DeepMid(0) catch (e) {
            _ => 1,
        };
        let b = DeepIndex([1, 2]) catch_all_panics (e) {
            _ => 1,
        };
        let c = ThrowingClosureChain(3) catch (e) {
            _ => 1,
        };
        let d = AwaitThrowMid(spawn { SpawnLeafThrow(1) }, 4, 99) catch (e) {
            _ => 1,
        };
        caught += a + b + c + d;
        i += 1;
    }
    let after = Measure();
    let inside = DeepMid(0) catch (e) {
        _ => Measure(),
    };
    baml.io.println(`${before > 1000}:${Near(after, before)}:${Near(inside, before)}:${caught}`);
}

// Whether two measured depths differ by less than a handful of frames.
function Near(a: int, b: int) -> bool {
    a - b < 4 && b - a < 4
}

function MapDown(d: Depths, n: int) -> int {
    d.deepest = n;
    let xs = [n].map((v: int) -> int { MapDown(d, v + 1) });
    xs[0]
}

function MapThrow(n: int) -> int {
    if (n == 0) {
        throw "map-bottom";
    }
    let xs = [n].map((v: int) -> int { MapThrow(v - 1) + 1 });
    xs[0]
}

// Recursion and errors through a builtin's callback: the VM call path and
// direct calls alternate on the way down.
function CallbackDepth() -> void {
    let d = Depths { deepest: 0 };
    let r = MapDown(d, 0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    let t = MapThrow(6) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}:${d.deepest > 100}`);
    baml.io.println(`${t}`);
}

interface Stepper {
    function step(self, n: int) -> int throws string;
}

class Down {
    limit: int,
    implements Stepper {
        function step(self, n: int) -> int throws string {
            if (n >= self.limit) {
                throw `down:${n}`;
            }
            n + 1
        }
    }
}

class Up {
    by: int,
    implements Stepper {
        function step(self, n: int) -> int throws string {
            n + self.by
        }
    }
}

function StepAll(steppers: Stepper[], n: int, rounds: int) -> int {
    if (rounds == 0) {
        return n;
    }
    let total = n;
    for (let s in steppers) {
        total = s.step(total);
    }
    StepAll(steppers, total, rounds - 1)
}

// One call site dispatching to alternating implementations, deep, until one
// throws.
function InterfaceErrors() -> void {
    let ok = StepAll([Up { by: 1 }, Down { limit: 1000 }, Up { by: 2 }], 0, 20);
    let bad = StepAll([Up { by: 3 }, Down { limit: 40 }], 0, 50) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${ok}`);
    baml.io.println(`${bad}`);
}

function GenDepth<T>(n: int) -> string {
    if (n == 0) {
        return TypeName<T>();
    }
    GenDepth<T>(n - 1)
}

function GenThrow<T>(n: int) -> int {
    if (n == 0) {
        throw `gen:${TypeName<T>()}`;
    }
    GenThrow<T>(n - 1) + 1
}

function GenList<T>(x: T, n: int) -> T[] {
    if (n == 0) {
        return [x];
    }
    let rest = GenList<T>(x, n - 1);
    rest.push(x);
    rest
}

// Generic chains with static type arguments (frame records) and a type
// declared at run time (VM frames), interleaved, with an error at the bottom.
function GenericChains() -> void {
    let rt = reflect.Class.new("Rt", { "v": int.type() });
    let type Rt = rt;
    let a = GenDepth<int>(12);
    let b = GenDepth<Point>(12);
    let c = GenDepth<Rt>(12);
    let d = GenThrow<string>(8) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    let f = GenThrow<Rt>(8) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    let xs = GenList<Point>(Point { x: 3 }, 30);
    baml.io.println(`${a}:${b}:${c}:${xs.length()}:${xs[29].x}`);
    baml.io.println(`${d}`);
    baml.io.println(`${f}`);
}

function HookedMid(counts: HookCounts, n: int) -> int {
    if (n == 0) {
        return chosen(counts, 5) + after_failure(counts);
    }
    HookedMid(counts, n - 1) + 1
}

/// baml:$trace=choose
function hooked_thrower(counts: HookCounts, n: int) -> int {
    counts.bodies += 1;
    DeepMid(n)
}

// Hooked callees below a chain of directly called bodies, and a hooked body
// whose direct callees throw.
function HookedDeep() -> void {
    let counts = HookCounts { hooks: 0, bodies: 0 };
    let a = HookedMid(counts, 10);
    let b = hooked_thrower(counts, 0) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${a}:${counts.hooks}:${counts.bodies}`);
    baml.io.println(`${b}`);
}

function LogChain(n: int) -> int {
    if (n == 0) {
        log.info("bottom of the chain");
        let f = (k: int) -> int {
            log.warn(`in closure ${k}`);
            k
        };
        return f(3);
    }
    LogChain(n - 1) + 1
}

function LogDefer(state: string[]) -> int {
    defer { log.info("defer while unwinding"); }
    DeferLeaf(state, 2)
}

// Log events from deep in a chain, a closure, and a defer during an unwind.
function LogsDeep() -> void {
    let a = LogChain(6);
    let state: string[] = [];
    let b = LogDefer(state) catch (e) {
        _ => -1,
    };
    baml.io.println(`${a}:${b}:${state.join(",")}`);
}

function UnwindSleepLeaf(state: string[]) -> int {
    defer {
        state.push("leaf-defer-start");
        baml.sys.sleep(baml.time.Duration.from_milliseconds(300));
        state.push("leaf-defer-end");
    }
    state.push("leaf");
    throw "leaf-error";
}

function UnwindSleepMid(state: string[]) -> int {
    defer { state.push("mid-defer"); }
    UnwindSleepLeaf(state) + 1
}

// A task is cancelled while a `defer` sleeps during an error's unwind below a
// chain.
function CancelDuringUnwind() -> void {
    let state: string[] = [];
    let child = spawn { UnwindSleepMid(state) };
    while (state.length() < 2) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    child.cancel();
    let _ = (await child) catch_all_panics (e) {
        _ => null,
    };
    baml.io.println(state.join(","));
}

// Stack overflow inside spawned tasks, several at once.
function TaskOverflow() -> void {
    let a = spawn { Measure() };
    let b = spawn { Measure() };
    let u = spawn { Descend(Depths { deepest: 0 }, 0) };
    let c = (await u) catch_all_panics (e) {
        _ => -7,
    };
    let depth_a = await a;
    let depth_b = await b;
    baml.io.println(`${depth_a > 1000}:${depth_a == depth_b}:${c}`);
}

function Wrap(n: int) -> int {
    let r = DeepMid(n) catch (e) {
        _ => {
            throw `wrapped:${n}`;
        },
    };
    r
}

function WrapMid(n: int) -> int {
    Wrap(n) + 1
}

// Errors rethrown from handlers in the middle of a chain: the cause chain and
// both traces.
function CauseChains() -> void {
    let r = WrapMid(1) catch (e, ctx) {
        _ => match (ctx.root_cause().error) {
            let s: string => `${ctx.error}|${s}|${Frames(ctx)}|${Frames(ctx.root_cause())}`,
            _ => "missing cause",
        },
    };
    baml.io.println(`${r}`);
}

function FastFail(n: int) -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    SpawnLeafThrow(n)
}

function Slow() -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(400));
    5
}

// `any` over a task that fails below a chain and one that succeeds later.
function AnyWithFailure() -> void {
    let r = (await baml.future.any([spawn { FastFail(3) }, spawn { Slow() }])) catch (e, ctx) {
        _ => -1,
    };
    baml.io.println(`${r}`);
}

function ClosureRecurse(n: int) -> int {
    let f = (k: int) -> int {
        if (k == 0) {
            let t = spawn { Work(3) };
            let v = await t;
            throw `closure-bottom:${v}`;
        }
        ClosureRecurse(k - 1) + 1
    };
    f(n)
}

// A spawned closure that recurses, suspends at the bottom, then throws.
function SpawnedClosures() -> void {
    let t = spawn { ClosureRecurse(12) };
    let r = (await t) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}`);
}

function ActivateHere(s: trace.Span) -> string {
    s._activate();
    baml.json.stringify(trace.current_context())
}

// A span activated by a directly called body is scoped to that body, as it
// is to the body's VM frame in bytecode.
function ManualSpan() -> void {
    let s = trace.start_span("probe-span", metadata = { "k": "v" });
    let before = baml.json.stringify(trace.current_context());
    let inside = ActivateHere(s);
    let after = baml.json.stringify(trace.current_context());
    let again = ActivateHere(s) catch (e) {
        _ => "second activation failed",
    };
    baml.io.println(`${before}|${inside}|${after}|${again}`);
    s.close();
}
