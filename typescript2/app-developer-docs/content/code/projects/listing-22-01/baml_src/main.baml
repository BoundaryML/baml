// docs:start all
class Usage {
    input: int,
    output: int,
}

class Reply {
    text: string,
    usage: Usage,
}

function cost(reply: Reply) -> int {
    let Reply { usage: Usage { input, output: let completion } } = reply;
    input + completion * 4
}
// docs:end all

test "class destructuring" {
    let reply = Reply { text: "Hello", usage: Usage { input: 120, output: 30 } };
    assert.equal(cost(reply), 240)
}
