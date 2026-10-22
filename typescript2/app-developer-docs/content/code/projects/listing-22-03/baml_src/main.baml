// docs:start all
class Step {
    position: int,
    label: string,
}

function advance(position: int) -> Step {
    Step { position: position + 2, label: `at ${position}` }
}
// docs:end all

// docs:start same-scope
function first_label() -> string {
    let position = 0;
    let Step { position, label } = advance(position);
    `${label}, now at ${position}`
}
// docs:end same-scope

// docs:start loop
function walk() -> string {
    let position = 0;
    let labels: string[] = [];
    while (position < 6) {
        let Step { position: let next, label } = advance(position);
        position = next;
        labels.push(label);
    }
    labels.join(", ")
}
// docs:end loop

test "destructuring into existing variables" {
    assert.equal(first_label(), "at 0, now at 2");
    assert.equal(walk(), "at 0, at 2, at 4")
}
