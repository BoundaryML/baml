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

function Recurse(n: int) -> int {
    1 + Recurse(n + 1)
}

function Recursion() -> void {
    let result = Recurse(0) catch_all_panics (e) {
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
