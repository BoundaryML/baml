// A `//#` header is a statement. After a block's final expression it makes
// that expression a statement too, so a block that must produce a value has
// none: the header does not attach to the value before it.

function header_after_value() -> int {
    let x = 4;
    x + 1
    //# done
}
