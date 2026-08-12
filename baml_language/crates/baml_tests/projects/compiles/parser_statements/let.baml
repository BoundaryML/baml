function let_statements() -> int {
    // Inferred type let
    let x = 1;

    // Typed let
    let y: int = 2;

    // Array typed let
    let z: int[] = [1, 2, 3];

    // Object typed let
    let point: Point = Point {
        x: 1,
        y: 2,
    };

    return point.x + point.y;
}

class Point {
    x int
    y int
}