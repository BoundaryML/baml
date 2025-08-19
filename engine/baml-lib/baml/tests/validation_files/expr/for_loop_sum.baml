fn Sum(xs: int[]) -> int {
    let mut result = 0;

    for x in xs {
        result += x;
    }

    result
}

fn main() -> int {
    Sum([1, 2, 3, 4])
}