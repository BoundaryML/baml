// Native ports of Phase-1 Rust integration tests.

test "wildcard_future_error_is_inferred_and_runs" {
    let result = root.mixed_phase1_support.call(
        "\nclass MyErr { msg string }\nfunction slow(x: int) -> int throws MyErr {\n  if (x < 0) { throw MyErr { msg: \"neg\" }; }\n  x\n}\n\nfunction main() -> int {\n  let fs: baml.future.Future<int, _>[] = [spawn { slow(1) }];\n  await fs[0]\n}\n",
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 1)
}

test "wildcard_map_value_is_inferred" {
    let result = root.mixed_phase1_support.call(
        "\n        function main() -> int {\n          let m: map<string, _> = { \"a\": 10, \"b\": 20 };\n          m.get(\"b\") ?? 0\n        }\n    ",
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 20)
}

test "wildcard_nested_union_member_is_inferred" {
    let result = root.mixed_phase1_support.call(
        "class Box<T> { v T }\nfunction main() -> int | string { let b: Box<int | _> = Box<int|string> { v: 1 }; b.v }",
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 1)
}

test "throws_wildcard_absorbs_stdlib_throws" {
    let result = root.mixed_phase1_support.call(
        "\n        class BadInput { msg string }\n        function parse(s: string) -> int throws BadInput | _ {\n          let v: json = baml.json.from_string(s);\n          throw BadInput { msg: \"nope\" };\n        }\n        function main() -> int { 0 }\n    ",
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.int_result(result), 0)
}
