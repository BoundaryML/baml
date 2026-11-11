function repro() -> int {
    let g = baml.spawn.Limit.new(2);
    let f = spawn with g {
        42
    };
    await f
}
