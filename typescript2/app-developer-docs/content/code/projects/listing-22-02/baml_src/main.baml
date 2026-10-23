// docs:start all
function describe_command(parts: string[]) -> string {
    let [let tool, let argument, ..] = parts else {
        return "expected a tool and an argument";
    };
    `${tool}(${argument})`
}
// docs:end all

test "array destructuring" {
    assert.equal(describe_command(["search", "baml", "--limit"]), "search(baml)");
    assert.equal(describe_command(["search"]), "expected a tool and an argument")
}
