// Newline boundaries depend on the whitespace after a leading minus, never indentation.
class Memory {
    suggest bool
    sessions int
    people int
}

function subtraction_key(s: Memory) -> int {
    let key = (s: Memory) -> int {
        (
            if (s.suggest) { 0 } else { 1000000 }
        )
            - s.sessions * 1000
            - s.people
    };
    key(s)
}

function grouped_subtraction_key(s: Memory) -> int {
    let key = (s: Memory) -> int {
        (
            if (s.suggest) { 0 } else { 1000000 }
        )
            - (s.sessions * 1000)
            - s.people
    };
    key(s)
}

function negation_key(s: Memory) -> int {
    let key = (s: Memory) -> int {
        (
            if (s.suggest) { 0 } else { 1000000 }
        )
            -(s.sessions * 1000)
            - s.people
    };
    key(s)
}

function guard(x: int?) -> int {
    if (x != null) { return x; }
    -1
}

function match_initializer(x: int) -> int {
    let y = match (x) {
        0 => 10,
        _ => 20,
    }
    -y
}

function values() -> int[] { [7, 8, 9] }
function record() -> int { 42 }
function consume(f: (int) -> int) -> int { f(3) }
function consume_list(f: (int) -> int[]) -> int[] { f(3) }

function parenthesized_tail() -> int {
    consume((x: int) -> int {
        record()
        (-x)
    })
}

function negative_tail() -> int {
    consume((x: int) -> int {
        record()
        -x
    })
}

function list_tail() -> int[] {
    consume_list((x: int) -> int[] {
        values()
        [0]
    })
}

function pick(c: bool) -> int[] {
    if (c) { [7, 8, 9] } else { [4, 5, 6] }
    [0]
}

function initializer(c: bool) -> int {
    let n = if (c) { 10 } else { 20 }
        - 3
    n
}

function assignment(c: bool) -> int {
    let n = 0;
    n = if (c) { 10 } else { 20 }
        - 3
    n
}

function returned(c: bool) -> int {
    return (if (c) { 10 } else { 20 })
        - 3
}

function incomplete(a: int, b: int) -> int {
    a -
    -b
}

function associativity(a: int, b: int, c: int) -> int {
    a
        - b
        - c
}

function negative_arm(x: int) -> int {
    match (x) {
        0 => 10
        -1 => 2
        _ => 3
    }
}

function verify(c: bool) -> bool {
    let s = Memory { suggest: c, sessions: 1, people: 1 };
    let expected = if (c) { -1001 } else { 998999 };
    let smaller = if (c) { 7 } else { 17 };
    subtraction_key(s) == expected
        && grouped_subtraction_key(s) == expected
        && negation_key(s) == -1001
        && guard(null) == -1
        && guard(5) == 5
        && match_initializer(0) == -10
        && match_initializer(1) == -20
        && parenthesized_tail() == -3
        && negative_tail() == -3
        && list_tail() == [0]
        && pick(c) == [0]
        && initializer(c) == smaller
        && assignment(c) == smaller
        && returned(c) == smaller
        && incomplete(10, 3) == 13
        && associativity(10, 3, 2) == 5
        && negative_arm(0) == 10
        && negative_arm(-1) == 2
        && negative_arm(9) == 3
}

test "newline_parsing_rules" {
    assert.equal(verify(true), true)
    assert.equal(verify(false), true)
}
