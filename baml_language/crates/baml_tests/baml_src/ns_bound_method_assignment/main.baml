// B-1868: projection stores must materialize callables just like local bindings.
type F = (x: int) -> int throws never;

class Holder {
    f: F?,
}

class G {
    function twice(self, x: int) -> int {
        x * 2
    }
}

function field_assignment() -> int {
    let g = G {};
    let h = Holder { f: null };
    h.f = g.twice;
    h.f?.(3) ?? -1
}

test "field_assignment_invokes_stored_method" {
    assert.equal(field_assignment(), 6)
}

function map_assignment() -> int {
    let g = G {};
    let m: map<string, (x: int) -> int throws never> = {};
    m["a"] = g.twice;
    m["a"](4)
}

test "map_assignment_invokes_stored_method" {
    assert.equal(map_assignment(), 8)
}

function array_assignment() -> int {
    let g = G {};
    let values: F[] = [g.twice];
    values[0] = g.twice;
    values[0](5)
}

test "array_projection_invokes_stored_method" {
    assert.equal(array_assignment(), 10)
}

function apply(f: F) -> int {
    f(5)
}

function working_controls() -> int[] {
    let g = G {};
    let h = Holder { f: null };
    let t: F = g.twice;
    h.f = t;
    let literal = Holder { f: g.twice };
    [h.f?.(2) ?? -1, literal.f?.(3) ?? -1, apply(g.twice)]
}

test "typed_local_workaround_class_literal_and_argument" {
    assert.equal(working_controls(), [4, 6, 10])
}

class Counter {
    value: int,
    function add(self, x: int) -> int {
        self.value += x;
        self.value
    }
}

// A call-produced receiver exercises the stack-carry simulator as well as emission.
function receiver(evaluations: Counter, value: Counter) -> Counter {
    evaluations.value += 1;
    value
}

function capture_receiver() -> int[] {
    let evaluations = Counter { value: 0 };
    let original = Counter { value: 1 };
    let current = original;
    let h = Holder { f: null };
    h.f = receiver(evaluations, current).add;
    let m: map<string, F> = {};
    m["a"] = receiver(evaluations, current).add;
    current = Counter { value: 100 };
    original.value = 10;
    [h.f?.(2) ?? -1, m["a"](3), original.value, current.value, evaluations.value]
}

test "receiver_is_evaluated_once_and_retained_by_value" {
    assert.equal(capture_receiver(), [12, 15, 15, 100, 2])
}

class Box<T> {
    value: T,
    function describe(self, value: T) -> string {
        reflect.Type.of<T>().to_string() + ":" + self.value.to_string() + ":" + value.to_string()
    }
}

class GenericHolder<T> {
    f: ((value: T) -> string throws never)?,
}

function generic_assignment<T>(value: T) -> string[] {
    let box = Box { value: value };
    let h = GenericHolder<T> { f: null };
    h.f = box.describe;
    let m: map<string, (value: T) -> string throws never> = {};
    m["a"] = box.describe;
    [h.f?.(value) ?? "missing", m["a"](value)]
}

test "generic_receiver_frame_survives_storage" {
    assert.equal(generic_assignment<int>(7), ["int:7:7", "int:7:7"]);
    assert.equal(generic_assignment<string>("hi"), ["string:hi:hi", "string:hi:hi"])
}

class GenericMethodWorker {
    prefix: string,
    function describe<T>(self, value: T) -> string {
        self.prefix + ":" + reflect.Type.of<T>().to_string() + ":" + value.to_string()
    }
}

function invoke_description<T>(f: (value: T) -> string throws never, value: T) -> string {
    f(value)
}

function generic_method_assignment<T>(value: T) -> string[] {
    let worker = GenericMethodWorker { prefix: "receiver" };
    let h = GenericHolder<T> { f: null };
    h.f = worker.describe;
    let m: map<string, (value: T) -> string throws never> = {};
    m["a"] = worker.describe;
    let explicit = GenericHolder<T> { f: null };
    explicit.f = worker.describe<T>;
    let local: (value: T) -> string throws never = worker.describe;
    [
        h.f?.(value) ?? "missing",
        m["a"](value),
        explicit.f?.(value) ?? "missing",
        local(value),
        invoke_description(worker.describe, value),
    ]
}

test "generic_method_on_non_generic_receiver_survives_storage" {
    assert.equal(
        generic_method_assignment<int>(7),
        ["receiver:int:7", "receiver:int:7", "receiver:int:7", "receiver:int:7", "receiver:int:7"],
    );
    assert.equal(
        generic_method_assignment<string>("hi"),
        [
            "receiver:string:hi",
            "receiver:string:hi",
            "receiver:string:hi",
            "receiver:string:hi",
            "receiver:string:hi",
        ],
    )
}

class GenericMethodBox<T> {
    value: T,
    function describe<U>(self, other: U) -> string {
        reflect.Type.of<T>().to_string()
            + ":"
            + self.value.to_string()
            + ":"
            + reflect.Type.of<U>().to_string()
            + ":"
            + other.to_string()
    }
}

function make_generic_method_box() -> GenericMethodBox<int> {
    GenericMethodBox { value: 7 }
}

test "generic_method_arguments_append_to_receiver_arguments" {
    let h = GenericHolder<string> { f: null };
    h.f = make_generic_method_box().describe;
    assert.equal(h.f?.("hi"), "int:7:string:hi")
}

interface Transform {
    function transform(self, x: int) -> int throws never;
}

implements Transform for Counter {
    function transform(self, x: int) -> int { self.value + x }
}

function interface_assignment(value: Transform) -> int[] {
    let h = Holder { f: null };
    h.f = value.transform;
    let m: map<string, F> = {};
    m["a"] = value.transform;
    [h.f?.(2) ?? -1, m["a"](3)]
}

test "interface_bound_method_survives_storage" {
    assert.equal(interface_assignment(Counter { value: 10 }), [12, 13])
}

type Throwing = (x: int) -> int throws string;

class ThrowingHolder {
    f: Throwing?,
}

class Worker {
    factor: int,
    function run(self, x: int) -> int throws string {
        if (x < 0) {
            throw "negative"
        }
        self.factor * x
    }
}

function throwing_assignment() -> int[] {
    let worker = Worker { factor: 3 };
    let h = ThrowingHolder { f: null };
    h.f = worker.run;
    let m: map<string, Throwing> = {};
    m["a"] = worker.run;
    let good = h.f?.(4) ?? -1;
    let field_error = h.f?.(-1) catch (e) {
        "negative" => -2,
        _ => -3
    };
    let map_error = m["a"](-1) catch (e) {
        "negative" => -4,
        _ => -5
    };
    [good, field_error ?? -6, map_error]
}

test "stored_method_preserves_result_and_throws_contract" {
    assert.equal(throwing_assignment(), [12, -2, -4])
}

function closure_assignment() -> int[] {
    let x = 1;
    let h = Holder { f: null };
    h.f
        = (n: int) -> int {
            x += n;
            x
        };
    let m: map<string, F> = {};
    m["a"]
        = (n: int) -> int {
            x += n;
            x
        };
    x = 10;
    [h.f?.(2) ?? -1, m["a"](3), x]
}

test "projection_stores_preserve_closure_cells" {
    assert.equal(closure_assignment(), [12, 15, 15])
}
