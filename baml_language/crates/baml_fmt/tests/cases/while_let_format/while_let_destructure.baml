class Item {
    value: int,
}

function f(it: Item | null) -> int {
    while let Item { value } = it {
        break;
    }
    0
}
