// Probes for escape to stack: objects that never leave their function become
// Rust structs in compiled code. Every probe prints what it observes, so a
// stack object that behaves differently from a heap one shows up as a diff.

class StackVec {
    x: int
    y: int

    function norm2(self) -> int {
        self.x * self.x + self.y * self.y
    }

    function bump(self, d: int) -> int {
        self.x += d;
        self.x
    }

    // Writes a field, then throws: the caller sees the write.
    function checked(self, limit: int) -> int {
        self.y = limit;
        if (self.x > limit) {
            throw `over:${self.x}`;
        }
        self.x
    }

    // Recursive, so it may suspend: its receiver stays on the heap.
    function depth(self, n: int) -> int {
        if (n == 0) {
            self.x
        } else {
            self.depth(n - 1) + 1
        }
    }
}

class StackTagged {
    name: string
    weight: float
    seen: bool
}

class StackFinal {
    n: int

    function cleanup(self) -> void {
        baml.io.println(`cleanup:${self.n}`);
    }
}

function StackDot(a: StackVec, b: StackVec) -> int {
    a.x * b.x + a.y * b.y
}

function StackKeep(v: StackVec, into: StackVec[]) -> int {
    into.push(v);
    into.length()
}

function StackMake(x: int) -> StackVec {
    StackVec { x: x, y: x + 1 }
}

// A scalar object per iteration, and a method on it.
function StackLoop() -> void {
    let acc = 0;
    for (let i = 0; i < 1000; i += 1) {
        let v = StackVec { x: i, y: i + 1 };
        acc += v.norm2();
    }
    baml.io.println(`loop:${acc}`);
}

// A method writes its receiver's fields; the caller reads them back.
function StackMutate() -> void {
    let v = StackVec { x: 1, y: 2 };
    let alias = v;
    let total = 0;
    for (let i = 0; i < 50; i += 1) {
        total += alias.bump(i);
    }
    v.y = v.x * 2;
    baml.io.println(`mutate:${total}:${v.x}:${v.y}:${alias.y}`);
}

// Stored into an array: stays on the heap, and keeps its identity.
function StackStored() -> void {
    let kept: StackVec[] = [];
    for (let i = 0; i < 10; i += 1) {
        let v = StackVec { x: i, y: i };
        kept.push(v);
        v.y = 100 + i;
    }
    baml.io.println(`stored:${kept.length()}:${kept[3].y}:${kept[9].norm2()}`);
}

// A callee's fresh object is returned: it stays on the heap.
function StackReturned() -> void {
    let v = StackMake(4);
    baml.io.println(`returned:${v.x}:${v.y}:${v.norm2()}`);
}

// Equality is deep; identity is not: separately built equal objects are
// equal but never the same object.
function StackIdentity() -> void {
    let a = StackVec { x: 1, y: 2 };
    let b = StackVec { x: 1, y: 2 };
    let c = StackVec { x: 3, y: 4 };
    let pool = [a];
    baml.io.println(`identity:${a == b}:${pool.includes(b)}:${pool.includes(a)}:${c.norm2()}`);
}

// A class with a finalizer stays on the heap.
function StackCleanup() -> void {
    let r = StackFinal { n: 3 };
    r.n += 1;
    r.cleanup();
    r.cleanup();
    baml.io.println(`final:${r.n}`);
}

// Passed to a callee that keeps it: stays on the heap.
function StackEscapingCallee() -> void {
    let into: StackVec[] = [];
    let v = StackVec { x: 5, y: 6 };
    let n = StackKeep(v, into);
    v.x = 50;
    baml.io.println(`escaping:${n}:${into[0].x}`);
}

// Passed to a callee that only reads it: both stay on the stack.
function StackPureCallee() -> void {
    let total = 0;
    for (let i = 0; i < 100; i += 1) {
        let a = StackVec { x: i, y: 2 };
        let b = StackVec { x: 3, y: i };
        total += StackDot(a, b);
    }
    baml.io.println(`pure:${total}`);
}

function StackWork(n: int) -> int {
    let parts: string[] = [];
    for (let i = 0; i < n; i += 1) {
        parts.push(`p${i}`);
    }
    parts.length()
}

// Heap fields of a stack object live across an await: compiled code spills
// and reloads them, so a moving collection cannot leave them stale.
function StackAwait() -> void {
    let out = "";
    let w = 0.5;
    for (let i = 0; i < 20; i += 1) {
        w = w + 1.25;
        let t = StackTagged { name: `name-${i}`, weight: w, seen: false };
        let child = spawn { StackWork(200) };
        let n = await child;
        t.seen = n == 200;
        t.weight = t.weight * 2.0;
        out = out + `${t.name}/${t.weight}/${t.seen};`;
    }
    baml.io.println(`await:${out}`);
}

function Trail(ctx: baml.errors.Context) -> string {
    let out = "";
    for (let frame in ctx.stack_trace.frames) {
        out = out + `${frame.function_name}@${frame.line};`;
    }
    out
}

// A throw from inside a method called on a stack object, caught by the
// caller: the same error, trace and field writes as on the heap.
function StackThrow() -> void {
    let v = StackVec { x: 7, y: 0 };
    let first = v.checked(10) catch (e, ctx) {
        _ => -1,
    };
    let second = v.checked(3) catch (e, ctx) {
        _ => {
            baml.io.println(`caught:${ctx.error}|${Trail(ctx)}`);
            -1
        },
    };
    baml.io.println(`throw:${first}:${second}:${v.y}`);
}

// A recursive method on an object.
function StackRecursive() -> void {
    let v = StackVec { x: 2, y: 0 };
    baml.io.println(`recursive:${v.depth(20)}`);
}

// Eligible on one branch, escaping on the other: the object stays on the
// heap. An object built on either branch and only read stays on the stack.
function StackBranch() -> void {
    let kept: StackVec[] = [];
    let acc = 0;
    for (let i = 0; i < 20; i += 1) {
        let v = StackVec { x: i, y: 1 };
        if (i % 3 == 0) {
            kept.push(v);
        } else {
            acc += v.norm2();
        }
        let w = if (i % 2 == 0) { StackVec { x: i, y: 0 } } else { StackVec { x: 0, y: i } };
        acc += w.x - w.y;
    }
    baml.io.println(`branch:${acc}:${kept.length()}:${kept[2].x}`);
}

// A type test and a null test on a stack object fold.
function StackTests() -> void {
    let v = StackVec { x: 1, y: 1 };
    let maybe: StackVec | null = null;
    let tested = match (v) {
        let s: StackVec => s.x + 10,
    };
    baml.io.println(`tests:${tested}:${v == null}:${null == v}:${maybe == null}`);
}
