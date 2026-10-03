// Repro: awaiting inside a `.map()` lambda (closure invoked by a native
// `YieldToCall` continuation). The lambda param must survive the await
// suspend/resume.

function one() -> int {
    1
}

function two() -> int {
    2
}

function three() -> int {
    3
}

function main() -> int {
    let fs = [spawn { one() }, spawn { two() }, spawn { three() }];
    let rs = fs.map((f) -> {
        await f
    });
    rs[0] * 100 + rs[1] * 10 + rs[2]
}

test "await_inside_map_lambda" {
    assert.equal(main(), 123)
}
