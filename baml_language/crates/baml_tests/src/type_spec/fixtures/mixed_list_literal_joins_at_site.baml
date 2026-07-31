//
// doc-inference.md ruling 1: joins happen at constraint-GENERATION sites -
// a container literal is one - so `[1, "x"]` types as `(int | string)[]`
// (the union is a real type; fresh literals widen at the binding). Only
// var RESOLUTION follows the equality regime; this is not a var.
function f() -> (int | string)[] {
    let a = [1, "x"];
     // ^ (int | string)[]
    a
}
