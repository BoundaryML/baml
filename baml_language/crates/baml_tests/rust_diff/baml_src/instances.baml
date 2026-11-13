// Class instances hold `int`, `float` and `bool` fields raw, and every other
// field as a value. Compiled bodies reach a field of a known class at its
// slot; these probes check they agree with bytecode on every kind, through
// collections (`BAML_GC_STRESS`), equality, rendering and reflection.

class Particle {
    x: float,
    count: int,
    alive: bool,
    label: string,
    weight: float?,
}

function Drift(p: Particle, steps: int) -> Particle {
    let i = 0;
    while (i < steps) {
        p.x = p.x * 1.0001 + 0.25;
        p.count += 3;
        p.alive = !p.alive;
        i += 1;
    }
    p
}

function InstanceFields() -> void {
    let p = Drift(Particle { x: 1.5, count: 2, alive: true, label: "p", weight: null }, 1001);
    p.weight = p.x / 2.0;
    baml.io.println(`fields:${p.x}:${p.count}:${p.alive}:${p.label}:${p.weight ?? -1.0}`);
    let q = Particle { x: -0.0, count: -5, alive: false, label: "q", weight: 1.0 };
    let same = Particle { x: 0.0, count: -5, alive: false, label: "q", weight: 1.0 };
    baml.io.println(`equal:${p == q}:${q == same}`);
    baml.io.println(`json:${baml.json.stringify(q)}`);
    let many: Particle[] = [];
    let i = 0;
    while (i < 2000) {
        many.push(Particle { x: i * 0.5, count: i, alive: i % 2 == 0, label: "m", weight: null });
        i += 1;
    }
    let total = 0.0;
    for (let m in many) {
        total += m.x + m.count;
    }
    baml.io.println(`many:${total}`);
}

class Tagged<T> {
    value: T,
    weight: float,
}

function InstanceGeneric() -> void {
    let tagged = Tagged { value: "seven", weight: 0.5 };
    tagged.weight *= 3.0;
    let boxed = Tagged { value: 7, weight: 2.0 };
    baml.io.println(`generic:${tagged.value}:${tagged.weight}:${boxed.value + 1}`);
    baml.io.println(`is:${tagged is Tagged<string>}:${boxed is Tagged<string>}`);
}

class Latched {
    log: string[],
    level: int,

    function cleanup(self) -> void {
        self.log.push(`cleaned:${self.level}`);
    }
}

function InstanceCleanupLatch() -> void {
    let r = Latched { log: [], level: 3 };
    r.cleanup();
    r.cleanup();
    {
        defer { r.cleanup(); }
        r.level = 4;
    }
    baml.io.println(`latch:${r.log.length()}:${r.log[0]}:${r.level}`);
}

class Measured {
    value: float,
}

function InstanceWrongKind() -> void {
    let refused = reflect.Class.instantiate<Measured>({ "value": 1 }) catch (e) {
        reflect.errors.InvalidArgument => {
            baml.io.println(`refused:${e.message}`);
            return;
        },
    };
    baml.io.println(`stored:${refused.value}`);
}
