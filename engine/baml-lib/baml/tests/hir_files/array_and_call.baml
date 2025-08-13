function Helper(x: int) -> int {
    x
}

function TestArray() -> int[] {
    let arr = [1, 2, 3];
    let result = Helper(42);
    [arr, result]
} 