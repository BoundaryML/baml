// fmt: line_width=60
function f() -> int {
    let g = baml.spawn.Limit.new(2);
    let h = spawn with baml.spawn.Limit.new(2), another_limit_value, third_one {
        42
    };
    await h
}
test "name" with FixtureNumberOne, FixtureNumberTwo, FixtureNumberThree {
    assert.is_true(true);
}
