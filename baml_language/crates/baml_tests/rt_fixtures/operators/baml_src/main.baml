function drv_add<T extends baml.ops.Add<T, Output = T>>(a: T, b: T) -> T { a + b }
function drv_sub<T extends baml.ops.Subtract<T, Output = T>>(a: T, b: T) -> T { a - b }
function drv_mul<T extends baml.ops.Multiply<T, Output = T>>(a: T, b: T) -> T { a * b }
function drv_div<T extends baml.ops.Divide<T, Output = T>>(a: T, b: T) -> T { a / b }
function drv_rem<T extends baml.ops.Remainder<T, Output = T>>(a: T, b: T) -> T { a % b }
function drv_neg<T extends baml.ops.Negate<Output = T>>(a: T) -> T { -a }
function drv_add_int_rhs<T extends baml.ops.Add<int, Output = T>>(a: T, b: int) -> T { a + b }
function drv_eq<T extends baml.ops.Equals>(a: T, b: T) -> bool { a.eq(b) }
function drv_add_method<T extends baml.ops.Add<T, Output = T>>(a: T, b: T) -> T { a.add(b) }

function RtNativeOperators() -> int {
    let ints = [drv_add(1, 2), drv_sub(1, 2), drv_mul(3, 4), drv_div(10, 3), drv_rem(10, 3), drv_neg(5), drv_add_method(4, 5)];
    let floats = [drv_add(1.5, 2.25), drv_div(1.0, 4.0), drv_neg(2.5), drv_rem(7.5, 2.0), drv_add_int_rhs(1.5, 2), drv_add_method(0.5, 0.25)];
    let big = [drv_add(10n, 3n), drv_div(10n, 3n), drv_neg(4n), drv_add_int_rhs(10n, 3)];
    log.info(ints);
    log.info(floats);
    log.info(big);
    log.info(drv_add("hello", " world"));
    log.info([drv_eq(1, 1), drv_eq(1.0, 2.0), drv_eq("a", "a"), drv_eq(true, false), drv_eq(3n, 3n)]);
    let overflow = drv_add(4611686018427387903, 1) catch (e) {
        baml.panics.IntegerOverflow => { log.warn("overflow"); 0 }
    };
    let zero = drv_div(1, 0) catch (e) {
        baml.panics.DivisionByZero => { log.warn("division by zero"); 0 }
    };
    ints[0] + overflow + zero
}

class HashPoint { x: int, label: string, tags: string[], score: float }
enum HashColor { Red, Green }

function hash_of<T extends baml.Hash>(value: T) -> int {
    let h = baml.hash.DefaultHasher.new();
    value.hash(h);
    h.finish()
}

function RtNativeHashing() -> int {
    let h = baml.hash.DefaultHasher.new();
    h.write(b"abc");
    let direct = h.finish();
    let values = [
        hash_of(1), hash_of("text"), hash_of(true), hash_of(null), hash_of(2.5), hash_of(0.0),
        hash_of([1, 2, 3]), hash_of({ "a": 1, "b": 2 }), hash_of({ "b": 2, "a": 1 }),
        hash_of(HashPoint { x: 1, label: "p", tags: ["a"], score: 1.5 }), hash_of(HashColor.Green),
        hash_of(10n), hash_of(b"xy"),
    ];
    log.info(direct);
    log.info(values);
    let p = HashPoint { x: 1, label: "p", tags: ["a"], score: 1.5 };
    let h2 = baml.hash.DefaultHasher.new();
    p.hash(h2);
    log.info(h2.finish());
    values.length()
}

