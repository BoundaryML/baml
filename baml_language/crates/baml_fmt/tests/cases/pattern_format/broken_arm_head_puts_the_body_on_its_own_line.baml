// fmt: line_width=40
function f(x: int) -> int {
    match (x) {
        1 if aaaaaaaaaaaaaa && bbbbbbbbbbbb && ccccccccccc => value_one,
        SomeLongVariant | AnotherLongVariant | Third => value_two,
        _ => 0,
    }
}
