interface I {}
class C { implements I {} }
function extra() -> I { C {}.as<I, MissingType> }
function empty() -> I { C {}.as<> }
function after() -> I { C {}.as<I> }
