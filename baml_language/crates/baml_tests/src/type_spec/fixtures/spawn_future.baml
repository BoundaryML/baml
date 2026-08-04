// A4 (S15 ledger): spawn expressions type as `Future<T, E>` - rustc's
// async-block shape. The body is a synthetic 0-arg lambda typed under
// its OWN effect channel (the S12 discipline); its fn type supplies
// both slots, fresh literals widened out. `await` distributes over
// future unions and contributes the error side to the effect channel.
class Boom {
    m string
}

function boom() -> int throws user.Boom {
    throw Boom { m: "x" }
}

function f() -> int throws never {
    let a = spawn { 42 };
     // ^ Future<int, never>
    let b = spawn { boom() };
     // ^ Future<int, user.Boom>
    0
}
