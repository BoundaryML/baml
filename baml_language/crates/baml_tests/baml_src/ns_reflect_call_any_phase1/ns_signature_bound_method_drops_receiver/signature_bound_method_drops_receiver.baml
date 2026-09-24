class Counter {
    base: int,

    function bump(self, amount: int) -> int throws never {
        self.base + amount
    }
}

function main() -> string throws never {
    let c = Counter { base: 40 };
    let m: reflect.AnyFunction = c.bump;
    let sig = reflect.signature(m);
    let method_name = "unnamed";
    let n = sig.name;
    if (n != null) {
        method_name = n
    }
    // A lambda has no source-level name, so it reports none.
    let lambda_name = "some";
    if (
        reflect
            .signature((x: int) -> int throws never {
                x
            })
            .name
            == null
    ) {
        lambda_name = "null"
    }
    // Also anchors the non-throwing spelling: `errors` reads `never`.
    return method_name
        + "|"
        + lambda_name
        + "|"
        + sig.args.length().to_string()
        + "|"
        + sig.returns.to_string()
        + "|"
        + sig.errors.to_string();
}

test "signature_bound_method_drops_receiver" {
    assert.equal(main(), "bump|null|1|int|never")
}
