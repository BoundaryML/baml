function f(a: Outer) -> int {
    let x = a.upcast<Show>(/* inside */).show();
    let y = a.upcast<Show> /* before */ ().show();
    let z = a.upcast<Show>() /* after */ .show();
    let w = a.upcast<Show>( // line
    ).show();
    x
}
