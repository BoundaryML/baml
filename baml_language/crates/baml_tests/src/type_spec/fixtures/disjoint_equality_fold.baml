// equality on compile-time-decidable disjoint singletons folds
function dfe_mixed() -> false throws never {
    5 == 5n
}
function dfe_widens() -> bool throws never {
    let r = 5 == 5n
     // ^ bool
    r
}
function dfe_outer() -> true throws never {
    (5 == 5n) == false
}
function dfe_null_literal() -> false throws never {
    null == 2.0
}
function dfe_null_null() -> true throws never {
    null == null
}
function dfe_ne() -> true throws never {
    5 != 5n
}
