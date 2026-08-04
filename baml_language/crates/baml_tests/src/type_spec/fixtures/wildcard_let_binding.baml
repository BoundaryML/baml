// A5 (S15 ledger): `let _` wildcard bindings turned out to WORK - the
// ledger's `!error` entries were cascades of `!error` INITIALIZERS
// (`let _ = waiter.cancel()` with Future methods unimplemented).
// Pinned as a regression fixture: the wildcard pattern types from the
// initializer and binds nothing.
function f(flag: bool) -> int throws never {
    let _ = flag;
//  ^^^^^ bool
    0
}
