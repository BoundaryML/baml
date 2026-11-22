class Invoice {}
class Receipt {}

function grouped(source: int | Receipt) -> int {
    let value: (int | string) | Invoice = source;
    0
}

function flat(source: int | Receipt) -> int {
    let value: int | string | Invoice = source;
    0
}
