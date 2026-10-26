// Each VM thread caches which impl of an interface a type uses. These tests
// warm that cache, change the impls it answered for, and check that dispatch
// follows: a session republish, a collection that moves the runtime class a
// cached answer names, and threads that dispatch while a republish lands.

// ─── a republish replaces warm answers ───────────────────────────────────────

test "warm_dispatch_follows_a_session_republish" {
    let session = reflect.Session.new()
    session.eval(`
        enum Tone { Low High }
        class Reading { tone Tone, level int }
        function same_tone(a: Tone, b: Tone) -> bool { a == b }
        function render(r: Reading) -> string { \`\${r.level}:\${r.tone}\` }
        function decode() -> int { baml.json.to<Reading>({ "tone": "Low", "level": 3 }).level }
        function warm() -> int {
            let hits = 0;
            for (let i = 0; i < 200; i += 1) {
                if (same_tone(Tone.Low, Tone.High)) { hits += 1; }
                hits += render(Reading { tone: Tone.Low, level: i }).length() + decode();
            }
            hits
        }
    `)
    // Each round renders "<level>:Low" and decodes level 3: 10 * 8 + 90 * 9 + 100 * 10.
    assert.equal(session.eval<int>(`warm()`), 1890)
    session.eval(`
        implements baml.ops.Equals for Tone {
            function eq(self, other: Self) -> bool { true }
        }
        implements baml.ToString for Reading {
            function to_string(self) -> string { "reading" }
        }
        implements baml.ToString for Tone {
            function to_string(self) -> string { "tone" }
        }
        implements baml.FromJson for Reading {
            function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
                Reading { tone: Tone.High, level: 40 }
            }
        }
    `)
    assert.equal(session.eval<bool>(`same_tone(Tone.Low, Tone.High)`), true)
    assert.equal(session.eval<string>(`render(Reading { tone: Tone.Low, level: 7 })`), "7:tone")
    assert.equal(session.eval<int>(`decode()`), 40)
}

// ─── a collection keeps warm answers about live runtime classes ──────────────

interface IrcBadge {
    label: string
    function shout(self) -> string throws never { self.label + "!" }
}

function irc_badge(label: string) -> IrcBadge {
    let minted = reflect.Class.new(
        "Badge",
        { "label": string.type() },
        implementations = [reflect.Interface.implementation<IrcBadge>().field("label")],
    );
    let type Badge = minted;
    let badge: Badge = baml.json.to<Badge>({ "label": label });
    match (badge) {
        let b: IrcBadge => b,
        _ => throw "the minted class does not implement IrcBadge",
    }
}

function irc_shout_many(badge: IrcBadge, n: int) -> int {
    let total = 0;
    for (let i = 0; i < n; i += 1) {
        total += badge.shout().length() + badge.label.length();
    }
    total
}

test "warm_dispatch_on_a_runtime_class_survives_collection" {
    let badge = irc_badge("gold");
    assert.equal(irc_shout_many(badge, 100), 900)
    // Churn the young heap so the minted class, its witness and the badge
    // move, then collect everything.
    for (let round = 0; round < 3; round += 1) {
        let garbage: string[][] = [];
        for (let i = 0; i < 2000; i += 1) {
            garbage.push(["x", "y", "z"]);
        }
        baml.sys.collect_garbage();
    }
    assert.equal(irc_shout_many(badge, 100), 900)
    // A class minted after the collection gets its own answers.
    assert.equal(irc_shout_many(irc_badge("tin"), 10), 70)
}

// Comparing two session values from this package keys the answer under the
// session's package, a heap object a collection moves.
function irc_churn_and_collect() -> null {
    for (let round = 0; round < 3; round += 1) {
        let garbage: string[][] = [];
        for (let i = 0; i < 2000; i += 1) {
            garbage.push(["x", "y", "z"]);
        }
        baml.sys.collect_garbage();
    }
    null
}

test "warm_dispatch_on_session_values_survives_collection" {
    let session = reflect.Session.new()
    session.eval(`class Value { n int }`)
    let a = session.eval<unknown>(`Value { n: 1 }`)
    let b = session.eval<unknown>(`Value { n: 2 }`)
    for (let i = 0; i < 50; i += 1) {
        assert.is_true(!baml.ops.equals_equals(a, b))
    }
    irc_churn_and_collect();
    assert.is_true(!baml.ops.equals_equals(a, b))
    assert.is_true(baml.ops.equals_equals(a, session.eval<unknown>(`Value { n: 1 }`)))
    session.eval(`
        implements baml.ops.Equals for Value {
            function eq(self, other: Self) -> bool { true }
        }
    `)
    assert.is_true(baml.ops.equals_equals(a, b))
}

// ─── a builder's witness lands while answers are warm ────────────────────────

interface IrcGate {}

interface IrcPick {
    function pick(self) -> string throws never { "picked" }
}

implements<T extends IrcGate> IrcPick for T {}

class IrcStaticGate {
    implements IrcGate {}
}

function irc_pick_of(value: unknown) -> string {
    match (value) {
        let p: IrcPick => p.pick(),
        _ => "none",
    }
}

test "a_builder_witness_registered_with_warm_answers_dispatches" {
    // Warm the blanket rule's answers through a static implementor.
    for (let i = 0; i < 50; i += 1) {
        assert.equal(irc_pick_of(IrcStaticGate {}), "picked")
    }
    assert.equal(irc_pick_of(1), "none")
    let builder = reflect.Class.builder("Gated");
    let built = builder.build(implementations = [reflect.Interface.implementation<IrcGate>()]);
    let type Gated = built;
    let gated: Gated = baml.json.to<Gated>({});
    assert.equal(irc_pick_of(gated), "picked")
    assert.is_true(built.as_type().implements(reflect.Type.of<IrcPick>()))
    assert.equal(irc_pick_of(IrcStaticGate {}), "picked")
}

// ─── threads dispatching while a republish lands ─────────────────────────────

class IrcWorker {
    warm: bool,
}

class IrcSignal {
    republished: bool,
}

function irc_all_warm(workers: IrcWorker[]) -> bool {
    for (let w in workers) {
        if (!w.warm) {
            return false;
        }
    }
    true
}

// Compare two session values until the main thread reports the republish,
// then once more. A comparison may see the new impl before the report; the
// one after the report must, since it starts after the republish finished.
function irc_compare_until(a: unknown, b: unknown, me: IrcWorker, signal: IrcSignal) -> bool {
    let rounds = 0;
    while (!signal.republished && rounds < 1000000) {
        if (!baml.ops.equals_equals(a, b)) {
            // This thread now holds a warm answer: the structural default.
            me.warm = true;
        }
        rounds += 1;
    }
    baml.ops.equals_equals(a, b)
}

test "threads_dispatching_through_a_republish_see_the_new_impl" {
    let session = reflect.Session.new()
    session.eval(`class Value { n int }`)
    let a = session.eval<unknown>(`Value { n: 1 }`)
    let b = session.eval<unknown>(`Value { n: 2 }`)
    let signal = IrcSignal { republished: false };
    let workers = [IrcWorker { warm: false }, IrcWorker { warm: false }, IrcWorker { warm: false }, IrcWorker { warm: false }];
    let w0 = spawn { irc_compare_until(a, b, workers[0], signal) };
    let w1 = spawn { irc_compare_until(a, b, workers[1], signal) };
    let w2 = spawn { irc_compare_until(a, b, workers[2], signal) };
    let w3 = spawn { irc_compare_until(a, b, workers[3], signal) };
    // Republish only once every worker holds a warm answer. The bound only
    // stops a hung run; workers warm within milliseconds.
    let waited = 0;
    while (!irc_all_warm(workers) && waited < 30000) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
        waited += 1;
    }
    assert.is_true(irc_all_warm(workers))
    session.eval(`
        implements baml.ops.Equals for Value {
            function eq(self, other: Self) -> bool { true }
        }
    `)
    signal.republished = true;
    assert.is_true(await w0)
    assert.is_true(await w1)
    assert.is_true(await w2)
    assert.is_true(await w3)
}
