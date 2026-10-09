// Native backend fixture: `catch` by class, by binding, wildcard arms, the
// panic guard, nested handlers, handlers in loops, and caught panics.

class Invalid { message: string }
class Coded { code: int, message: string }

function risky(x: int) -> int {
    if (x < 0) { throw Invalid { message: "negative" } }
    if (x == 0) { throw Coded { code: 7, message: "zero" } }
    x * 2
}

function middle(x: int) -> int {
    risky(x) + 1
}

function catch_class(x: int) -> int {
    risky(x) catch (e) { Invalid => -1 }
}

function catch_two(x: int) -> int {
    risky(x) catch (e) { Invalid => -1, Coded => -2 }
}

function catch_bind(x: int) -> string {
    risky(x).to_string() catch (e) {
        let i: Invalid => i.message,
        let c: Coded => c.message + c.code.to_string(),
    }
}

function catch_wild(x: int) -> int {
    risky(x) catch (e) { _ => -1 }
}

function catch_wild_panic(x: int) -> int {
    (10 / x) catch (e) { _ => -1 }
}

function catch_all_panic(x: int) -> int {
    (10 / x) catch_all (e) { _ => -1 }
}

function catch_all_panics(x: int) -> int {
    (10 / x) catch_all_panics (e) { _ => -1 }
}

function catch_let_wild(x: int) -> int {
    risky(x) catch (_) { let _ => -3 }
}

function catch_four(x: int) -> int {
    let xs = [1, 2, 3];
    let v = if (x > 100) { xs[x] } else { 10 / risky(x) };
    (v) catch (e) {
        Invalid => -1,
        Coded => -2,
        baml.panics.DivisionByZero => -3,
        baml.panics.IndexOutOfBounds => -4,
    }
}

function catch_panics(x: int) -> int {
    let xs = [1, 2, 3];
    let m: map<string, int> = { "1": 10 };
    let r = 0;
    r = r + ((10 / x) catch (e) { baml.panics.DivisionByZero => 100 });
    r = r + (xs[x] catch (e) { baml.panics.IndexOutOfBounds => 1000 });
    r = r + ((x * 4611686018427387903) catch (e) { baml.panics.IntegerOverflow => 10000 });
    r = r + (m[x.to_string()] catch (e) { baml.panics.MapKeyNotFound => 100000 });
    r
}

function catch_nested(x: int) -> int {
    (risky(x) catch (e) { Invalid => risky(x + 100) }) catch (e) { Invalid => -1, Coded => -2 }
}

function catch_deep(x: int) -> int {
    middle(x) catch (e) { Invalid => -1, Coded => -2 }
}

function catch_in_loop(n: int) -> int {
    let acc = 0;
    for (let x in [n, -1, 0, 3]) {
        let v = risky(x) catch (e) {
            Invalid => { continue },
            Coded => { break },
        };
        acc = acc + v;
    }
    acc
}

function catch_returns(n: int) -> int {
    let i = 0;
    while (i < 5) {
        risky(n - i) catch (e) { Invalid => { return i * 10 } };
        i = i + 1;
    }
    i
}

function catch_rethrow(x: int) -> int {
    risky(x) catch (e) { Coded => -2 }
}

function catch_throw_in_arm(x: int) -> int {
    risky(x) catch (e) { let i: Invalid => throw Coded { code: 1, message: i.message } }
}

function catch_throw_bound(x: int) -> int {
    risky(x) catch (e) { let i: Invalid => throw i }
}

function catch_throw_caught(x: int) -> int {
    risky(x) catch (e) { _ => throw e }
}

function catch_identity(x: int) -> int {
    let c = Coded { code: x, message: "m" };
    let r = { if (x < 0) { throw c }; 1 } catch (e) {
        let k: Coded => { k.code = k.code + 1; 0 }
    };
    c.code + r
}

function catch_statement(x: int) -> int {
    let r = 0;
    risky(x) catch (e) { Invalid => { r = 1 }, Coded => { r = 2 } };
    r
}

function catch_stdlib(x: int) -> int {
    let v = if (x < 0) { throw baml.errors.InvalidArgument { message: "neg" } } else { x };
    (v) catch (e) { baml.errors.InvalidArgument => -1 }
}

function catch_exit(x: int) -> int {
    let v = if (x < 0) { throw baml.panics.Exit { code: x } } else { x };
    (v) catch (e) { _ => -1 }
}
