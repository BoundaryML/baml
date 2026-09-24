// Native ports of Phase-1 Rust integration tests.

test "wildcard_future_error_is_inferred_and_runs" {
    let result = root.mixed_phase1_support.call(
        `
            class MyErr { msg string }
            function slow(x: int) -> int throws MyErr {
                if (x < 0) { throw MyErr { msg: "neg" }; }
                x
            }

            function main() -> int {
                let fs: baml.future.Future<int, _>[] = [spawn { slow(1) }];
                await fs[0]
            }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 1)
}

test "wildcard_map_value_is_inferred" {
    let result = root.mixed_phase1_support.call(
        `
            function main() -> int {
                let m: map<string, _> = { "a": 10, "b": 20 };
                m.get("b") ?? 0
            }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 20)
}

test "wildcard_nested_union_member_is_inferred" {
    let result = root.mixed_phase1_support.call(
        `
            class Box<T> { v T }
            function main() -> int | string {
                let b: Box<int | _> = Box<int | string> { v: 1 };
                b.v
            }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 1)
}

test "throws_wildcard_absorbs_stdlib_throws" {
    let result = root.mixed_phase1_support.call(
        `
            class BadInput { msg string }
            function parse(s: string) -> int throws BadInput | _ {
                let v: json = baml.json.from_string(s);
                throw BadInput { msg: "nope" };
            }
            function main() -> int { 0 }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 0)
}
