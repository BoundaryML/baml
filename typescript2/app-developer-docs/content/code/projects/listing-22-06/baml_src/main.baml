// docs:start for
class Usage {
    input: int,
    output: int,
}

function total_tokens(usages: Usage[]) -> int {
    let total = 0;
    for let Usage { input, output } in usages {
        total += input + output;
    }
    total
}
// docs:end for

// docs:start while
class Step {
    name: string,
    next: Step?,
}

function step_names(first: Step?) -> string[] {
    let names: string[] = [];
    let current = first;
    while let Step { name, next } = current {
        names.push(name);
        current = next;
    }
    names
}
// docs:end while

test "loop patterns" {
    assert.equal(total_tokens([Usage { input: 10, output: 2 }, Usage { input: 5, output: 1 }]), 18);
    let plan = Step { name: "search", next: Step { name: "read", next: Step { name: "answer", next: null } } };
    assert.equal(step_names(plan), ["search", "read", "answer"]);
    assert.equal(step_names(null), [])
}
