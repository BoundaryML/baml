// Adversarial probes for escape analysis: each shares an array between two
// tasks a different way, and both mutate it at once. Every access to it must
// keep its lock; debug builds panic on a lock-free access that finds the lock
// held. Results are order-independent, so every mode prints the same line.

class Holder {
    items: int[]
}

function PushN(xs: int[], n: int) -> int {
    let i = 0;
    while (i < n) {
        xs.push(i);
        i += 1;
    }
    xs.length()
}

function Total(xs: int[]) -> int {
    let total = 0;
    for (let x in xs) {
        total += x;
    }
    total
}

function Report(name: string, xs: int[]) -> void {
    baml.io.println(`${name}:${xs.length()}:${Total(xs)}`);
}

// Shared by capture in the spawned body.
function EscapeSpawn() -> void {
    let xs: int[] = [];
    let child = spawn { PushN(xs, 2000) };
    let i = 0;
    while (i < 2000) {
        xs.push(i);
        xs[0] = xs[0];
        i += 1;
    }
    await child;
    Report("spawn", xs);
}

function Launch(xs: int[], n: int) -> baml.future.Future<int, never> {
    spawn { PushN(xs, n) }
}

// Shared by a callee that spawns with its argument.
function EscapeSpawnInCallee() -> void {
    let xs: int[] = [];
    let child = Launch(xs, 2000);
    PushN(xs, 2000);
    await child;
    Report("callee", xs);
}

// Shared through a closure that a spawned body calls.
function EscapeClosure() -> void {
    let xs: int[] = [];
    let push = (n: int) -> int { PushN(xs, n) };
    let child = spawn { push(2000) };
    PushN(xs, 2000);
    await child;
    Report("closure", xs);
}

// Shared through a field of an object the spawned body reads.
function EscapeField() -> void {
    let xs: int[] = [];
    let holder = Holder { items: xs };
    let child = spawn { PushN(holder.items, 2000) };
    PushN(xs, 2000);
    await child;
    Report("field", xs);
}

function Stash(holder: Holder, xs: int[]) -> int {
    StashDeeper(holder, xs)
}

function StashDeeper(holder: Holder, xs: int[]) -> int {
    holder.items = xs;
    0
}

// Shared after a store two calls deep into an object the spawned body reads.
function EscapeStoredDeep() -> void {
    let xs: int[] = [];
    let holder = Holder { items: [] };
    Stash(holder, xs);
    let child = spawn { PushN(holder.items, 2000) };
    PushN(xs, 2000);
    await child;
    Report("deep", xs);
}

// Shared through a map value.
function EscapeMapValue() -> void {
    let xs: int[] = [];
    let m = {"a": xs};
    let child = spawn { PushN(m["a"], 2000) };
    PushN(xs, 2000);
    await child;
    Report("map", xs);
}

function CountInto(m: map<string, int>, n: int) -> int {
    let i = 0;
    while (i < n) {
        m.set(`k${i % 7}`, m.get_or_insert(`k${i % 7}`, 0) + 1);
        i += 1;
    }
    m.length()
}

// Shared by capture in the spawned body: both tasks update the map itself.
function EscapeMap() -> void {
    let m: map<string, int> = {};
    let child = spawn { CountInto(m, 2000) };
    CountInto(m, 2000);
    await child;
    let total = 0;
    for (let k in m.keys()) {
        total += m[k];
    }
    // Updates race at the BAML level, so only the key count is fixed.
    baml.io.println(`map-shared:${m.length()}:${total > 0}`);
}

// Shared through `insert` into an array the spawned body reads.
function EscapeInsert() -> void {
    let xs: int[] = [];
    let holder: int[][] = [];
    holder.insert(xs, 0);
    let child = spawn { PushN(holder[0], 2000) };
    PushN(xs, 2000);
    await child;
    Report("insert", xs);
}

function GrowAndTotal(xs: int[], n: int) -> int {
    PushN(xs, n);
    Total(xs)
}

// One function called with an array only this task can reach, and with one
// a spawned body mutates: the locality mask lets the first call, and the
// calls it makes with its parameter, skip the locks the second must take.
function EscapeMaskMixed() -> void {
    let mine: int[] = [];
    GrowAndTotal(mine, 1000);
    let xs: int[] = [];
    let child = spawn { PushN(xs, 2000) };
    GrowAndTotal(xs, 2000);
    await child;
    Report("mixed", xs);
}

// A closure's parameter arrives through an indirect call, whose locality
// mask is always 0, however local the argument looks at the call.
function EscapeMaskClosure() -> void {
    let xs: int[] = [];
    let grow = (ys: int[]) -> int { GrowAndTotal(ys, 2000) };
    let child = spawn { PushN(xs, 2000) };
    grow(xs);
    await child;
    Report("mask-closure", xs);
}

function Same(xs: int[]) -> int[] {
    xs
}

// Shared through an alias a callee returns.
function EscapeReturnedAlias() -> void {
    let xs: int[] = [];
    let alias = Same(xs);
    let child = spawn { PushN(alias, 2000) };
    PushN(xs, 2000);
    await child;
    Report("alias", xs);
}

function SpawnAtBottom(xs: int[], depth: int) -> baml.future.Future<int, never> {
    if (depth == 0) {
        spawn { PushN(xs, 2000) }
    } else {
        SpawnAtBottom(xs, depth - 1)
    }
}

// Shared by a recursive callee that spawns at its base case.
function EscapeRecursive() -> void {
    let xs: int[] = [];
    let child = SpawnAtBottom(xs, 5);
    PushN(xs, 2000);
    await child;
    Report("recursive", xs);
}

function Drain(it: baml.iter.Iterator<Item = int, Error = never>) -> int {
    let total = 0;
    for (let x in it) {
        total += x;
    }
    total
}

// Shared through an iterator: the spawned body walks the array while the
// parent reads it.
function EscapeIterator() -> void {
    let xs: int[] = [];
    PushN(xs, 4000);
    let it = xs.iter();
    let child = spawn { Drain(it) };
    let reads = 0;
    let i = 0;
    while (i < 4000) {
        reads += xs[i] - xs[i] + xs.length();
        i += 1;
    }
    let drained = await child;
    baml.io.println(`iterator:${drained}:${reads}`);
}

function BuildAndSum(n: int) -> int {
    let xs: int[] = [];
    let i = 0;
    while (i < n) {
        xs.push(i * 2);
        i += 1;
    }
    let j = 0;
    while (j < n) {
        xs[j] = xs[j] + 1;
        j += 1;
    }
    let m: map<string, int> = {};
    let k = 0;
    while (k < n) {
        m[`k${k % 64}`] = m.get_or_insert(`k${k % 64}`, 0) + xs[k];
        if (m.has("k0") && k % 5 == 0) {
            m.delete("k1");
        }
        k += 1;
    }
    Total(xs) + xs.length() + m.length() + (m.get("k0") ?? 0)
}

// Control: two tasks, each with arrays and maps only it can reach, which
// take the lock-free paths at the same time.
function ThreadLocalParallel() -> void {
    let a = spawn { BuildAndSum(5000) };
    let b = spawn { BuildAndSum(5000) };
    let mine = BuildAndSum(5000);
    let theirs = (await a) + (await b);
    baml.io.println(`parallel:${mine}:${theirs}`);
}
