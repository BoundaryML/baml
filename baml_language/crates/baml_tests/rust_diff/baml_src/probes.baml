function Scalars() -> void {
    let n = 0;
    let sum = 0;
    while (n < 100) {
        sum = sum + n * 3 - n / 2;
        n += 1;
    }
    baml.io.println(`${sum}`);
}

function Overflow() -> void {
    let n = 4611686018427387903;
    baml.io.println(`${n + 1}`);
}

function Containers() -> void {
    let xs = [1, 2, 3];
    xs.push(4);
    xs[0] = 9;
    let m = { "a": xs[0], "b": xs[3] };
    m["a"] = 10;
    baml.io.println(`${xs[0]}:${xs.length()}:${m["a"]}:${m["b"]}`);
}

enum Color {
    Red,
    Blue,
}

class Point {
    x: int,
}

class Label {
    text: string,
}

function Describe(value: Point | Label) -> string {
    match (value) {
        let p: Point => `point:${p.x}`,
        let l: Label => l.text,
    }
}

function Shapes() -> void {
    let color = Color.Blue;
    baml.io.println(
        `${Describe(Point { x: 7 })}:${Describe(Label { text: "label" })}:${color == Color.Blue}`,
    );
}

function Identity<T>(value: T) -> T {
    value
}

function Closures() -> void {
    let offset = 7;
    let f = (n: int) -> {
        Identity(n) + offset
    };
    let xs = [1, 2, 3].map(f);
    baml.io.println(`${xs[0]}:${xs[2]}:${Identity("generic")}`);
}

interface Scorer {
    function score(self, n: int) -> int throws never;
}

class Offset {
    base: int,
    implements Scorer {
        function score(self, n: int) -> int throws never {
            self.base + n
        }
    }
}

function Dispatch(s: Scorer) -> int {
    s.score(5)
}

function Interfaces() -> void {
    baml.io.println(`${Dispatch(Offset { base: 10 })}`);
}

function FailA() -> string {
    throw "A";
}

function FailB() -> string {
    throw "B";
}

function Rethrow() -> string {
    FailB() catch (e, ctx) {
        let n: int => "unexpected",
    }
}

function Contexts() -> void {
    let result = FailA() catch (e, ctx) {
        _ => {
            Rethrow() catch (second, context) {
                _ => {
                    match (context.root_cause().error) {
                        let s: string => `root:${s}`,
                        _ => "missing cause",
                    }
                },
            }
        },
    };
    baml.io.println(result);
}

function Uncaught() -> void {
    let _ = Rethrow();
}

function Async() -> void {
    let a = spawn { 20 };
    let b = spawn { 22 };
    let sum = await a + await b;
    let winner = await baml.future.any([spawn { 21 }, spawn { 21 }]);
    baml.io.println(`${winner}:${sum}`);
}

function Cancellation() -> void {
    let state: string[] = [];
    let child = spawn {
        defer { state.push("cleanup"); }
        state.push("ready");
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000));
    };
    while (state.length() == 0) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    child.cancel();
    let _ = (await child) catch_all_panics (e) {
        _ => null,
    };
    baml.io.println(`${state.length()}:${state[0]}:${state[1]}`);
}

function Fibonacci(n: int) -> int {
    if (n < 2) {
        return n;
    }
    Fibonacci(n - 1) + Fibonacci(n - 2)
}

function RecursiveWork() -> int {
    Fibonacci(25) + RecursiveWork()
}

// Recursive calls must yield to cancellation even without a loop backedge.
// Keep computing until cancellation so scheduler speed cannot win the race.
function RecursiveCancellation() -> void {
    let state: string[] = [];
    let child = spawn {
        defer { state.push("cleanup"); }
        state.push("ready");
        RecursiveWork()
    };
    while (state.length() == 0) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    child.cancel();
    let result = (await child) catch_all_panics (e) {
        let cancelled: baml.panics.Cancelled => -1,
        _ => -2,
    };
    baml.io.println(`${result}:${state.length()}:${state[0]}:${state[1]}`);
}

// The handler's allocation loop needs a safepoint even with no try-body call.
function CatchAllocation() -> void {
    let values: Point[] = [];
    let count = {
        throw "allocate";
    } catch (e) {
        _ => {
            let i = 0;
            while (i < 2000) {
                values.push(Point { x: i });
                i += 1;
            }
            values.length()
        },
    };
    baml.io.println(`${count}:${values[0].x}:${values[1999].x}`);
}

function Recurse(n: int) -> int {
    1 + Recurse(n + 1)
}

function Recursion() -> void {
    let result = Recurse(0) catch (e) {
        let e: baml.panics.StackOverflow => -1,
    };
    baml.io.println(`${result}`);
}

function Allocation() -> void {
    let keep = [Point { x: 17 }];
    let i = 0;
    let total = 0;
    while (i < 2000) {
        let points = [Point { x: i }, Point { x: i + 1 }];
        total += points[0].x + points[1].x;
        i += 1;
    }
    baml.io.println(`${keep[0].x}:${total}`);
}

function Logs() -> void {
    log.info("differential log");
    log.warn("differential warning");
    baml.io.println("logged");
}

function Sized(n: int) -> int {
    let s = "";
    for (let i = 0; i < n; i += 1) {
        s = s + "x";
    };
    s.length()
}

// A callee suspends and returns; its caller then enters another compiled
// body at the same frame depth, which suspends again.
function Depth() -> void {
    let first = string.from(Sized(3));
    let second = string.from(Sized(5));
    baml.io.println(`${first}:${second}`);
}

function SumIter(it: baml.iter.Iterator<Item = int, Error = never>) -> int {
    let total = 0;
    for (let v in it) {
        total += v;
    }
    total
}

// A plain loop walks the array by index; an iterator that escapes (passed
// on, adapted, or consumed after the source grew) is the real object. Both
// observe pushes to the source during iteration.
function Iterators() -> void {
    let xs = [1, 2, 3];
    let plain = 0;
    for (let v in xs) {
        plain += v;
    }
    let passed = SumIter(xs.iter());
    let chained = xs.iter().chain([4, 5]).map((v: int) -> int { v * 10 }).collect();
    let chained_total = 0;
    for (let v in chained) {
        chained_total += v;
    }
    let late = xs.iter();
    xs.push(4);
    let lazy = SumIter(late);
    let grown = [1, 2];
    let seen = 0;
    for (let v in grown) {
        if (grown.length() < 5) {
            grown.push(v + 10);
        }
        seen += v;
    }
    baml.io.println(
        `${plain}:${passed}:${chained.length()}:${chained_total}:${lazy}:${grown.length()}:${seen}`,
    );
}

function TypeName<T>() -> string {
    reflect.Type.of<T>().to_string()
}

// A type bound at run time (`let type`) is passed on by generic calls, and
// sibling bindings reuse one frame slot.
function BoundTypes() -> void {
    let type T = int.type();
    let held = match (21) {
        let n: T => Identity<T>(n) is int,
        _ => false,
    };
    baml.io.println(`${TypeName<T>()}:${held}`);
    let first: string = {
        let type A = string.type();
        TypeName<A>()
    };
    let second: string = {
        let type B = Point.type();
        TypeName<B>()
    };
    baml.io.println(`${first}:${second}`);
}

// A class declared at run time is bound as a type, parsed into and tested.
function RuntimeClass() -> void {
    let saved = reflect.Class.new("Saved", { "height": int.type(), "name": string.type() });
    let type Saved = saved;
    let text = {
        let parsed = baml.sap.parse<Saved>(`{"height": 180, "name": "Ada"}`);
        baml.json.to_string(parsed)
    } catch (e, ctx) {
        _ => "parse failed"
    };
    baml.io.println(text);
    let again = baml.sap.parse_type(saved.as_type(), `{"height": 1, "name": "b"}`) catch (e, ctx) {
        _ => null
    };
    baml.io.println(`${again is Saved}:${TypeName<Saved>()}`);
}

// `String.length`, `Range` and the iterator adapters carry `trace.hidden`
// hooks; their compiled bodies resolve them as bytecode's prologue does.
function Hidden() -> void {
    let words = ["a", "bb", "ccc", ""];
    let total = 0;
    let i = 0;
    while (i < words.length()) {
        total += words[i].length();
        i += 1;
    }
    let ranged = 0;
    for (let k in baml.iter.Range.new(0, 5)) {
        ranged += k;
    }
    let chained = [1, 2, 3, 4, 5, 6].iter().filter((x: int) -> bool {
        x % 2 == 0
    }).map((x: int) -> int {
        x * 10
    }).collect();
    baml.io.println(`${total}:${ranged}:${chained}`);
}

class HookCounts {
    hooks: int,
    bodies: int,
}

function choose(counts: HookCounts, n: int, settings: trace.Settings) -> trace.Options? throws never {
    counts.hooks += 1;
    trace.hidden()
}

/// baml:$trace=choose
function chosen(counts: HookCounts, n: int) -> int {
    counts.bodies += 1;
    n * 2
}

function failing_hook(counts: HookCounts, settings: trace.Settings) -> trace.Options? throws never {
    counts.hooks += 1;
    baml.sys.panic("hook failure")
}

/// baml:$trace=failing_hook
function after_failure(counts: HookCounts) -> int {
    counts.bodies += 1;
    7
}

function Hooked() -> void {
    let counts = HookCounts { hooks: 0, bodies: 0 };
    let a = chosen(counts, 21);
    let b = after_failure(counts);
    let c = chosen(counts, 4);
    baml.io.println(`${a}:${b}:${c}:${counts.bodies}`);
}

function Work(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        total += i;
        i += 1;
    }
    total
}

// Many short tasks, each on its own VM, run a compiled body to completion:
// the fiber stacks they release are reused by the tasks that follow.
function Spawns() -> void {
    let total = 0;
    let round = 0;
    while (round < 20) {
        let tasks: baml.future.Future<int, never>[] = [];
        let i = 0;
        while (i < 100) {
            let n = i;
            tasks.push(spawn { Work(n) });
            i += 1;
        }
        for (let task in tasks) {
            total += await task;
        }
        round += 1;
    }
    baml.io.println(`${total}`);
}

class Tree {
    left: Tree?,
    right: Tree?,
}

class Coin {
    value: int,
    label: string,
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.value == other.value
        }
    }
}

function Leaves(t: Tree?) -> int {
    if (t == null) {
        return 0;
    }
    let l = t.left;
    let r = t.right;
    if (l == null && r == null) {
        return 1;
    }
    Leaves(l) + Leaves(r)
}

// `==` with a `null` or primitive side runs in place; a pair that may reach
// a user `Equals.eq` (here `Coin`'s, which ignores `label`) still calls it.
function Equality() -> void {
    let t = Tree { left: Tree { left: null, right: null }, right: Tree { left: null, right: null } };
    let n: int? = 3;
    let s: string? = null;
    let a = Coin { value: 5, label: "a" };
    let b = Coin { value: 5, label: "b" };
    let c = Coin { value: 6, label: "a" };
    let maybe: Coin? = b;
    let none: Coin? = null;
    let u: unknown = a;
    baml.io.println(
        `${Leaves(t)}:${t.right == null}:${t.left != null}:${null == none}:${null != maybe}`,
    );
    baml.io.println(
        `${n == 3}:${n != null}:${s == null}:${s == "x"}:${u == 5}:${[1, 2] == [1, 2]}:${{"k": 1.5} == {"k": 1.5}}`,
    );
    baml.io.println(
        `${a == b}:${a == c}:${a != b}:${maybe == a}:${none == a}:${u == b}:${[a] == [b]}:${[a] == [c]}`,
    );
}
