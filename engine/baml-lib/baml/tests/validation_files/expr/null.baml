class Node {
    value int
    next Node?
}

function main() -> int {
    let a = Node { value: 1, next: null };
    let b = Node { value: 2, next: a };

    // Create a circular reference
    a.next = b;

    let copy = baml.deep_copy(a);

    // Modify the original
    a.value = 99;

    // The copy should be unchanged
    copy.value
}
