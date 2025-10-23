function A() -> int {

    let x = if (true) {
        1
    } else {
        2
    };

    if (true) {
      x = 10;
    } else {
      x = 20;
    };

    0
}
