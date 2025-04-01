fn Id(x: int) -> int {
    x
}

fn Go(x: int) -> int[] {
 [x, Id(x), Id(Id(x))]
}