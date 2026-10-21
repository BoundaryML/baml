class Counter {
    base: int,

    function bump(self, amount: int) -> int throws never {
        self.base + amount
    }
}

function main() -> int throws never {
    let c = Counter { base: 40 };
    let m: reflect.AnyFunction<Returns = int, Throws = never> = c.bump;
    return reflect.call_any(m, { "amount": 2 }) catch (e) {
        _ => -1
    };
}

test "call_any_bound_method_callee" {
    assert.equal(main(), 42)
}
