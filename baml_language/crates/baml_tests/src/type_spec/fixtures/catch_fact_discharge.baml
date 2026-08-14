// Catch as a throw-fact matcher
//
// catch discharges a SET of throw facts, not a value of a union type:
// arms match fact-by-fact (definite handling subtracts the fact, the
// residual rethrows), and an arm naming a `baml.panics.*` type is
// ALWAYS claimable - panics are catchable at runtime but never part of
// any `throws` contract, so they fold in per-arm beside the facts.
class CfNet {
    message string
}

class CfParse {
    key string
}

function cf_risky(x: int) -> int throws CfNet | CfParse {
    if (x > 0) {
        throw CfNet { message: "down" }
    }
    throw CfParse { key: "k" }
}

// Handles CfNet; the CfParse FACT survives subtraction and rethrows -
// the declared clause accepts exactly the residual.
function cf_partial(x: int) -> int throws CfParse {
    let n = cf_risky(x) catch (e) {
     // ^ int
        let net: CfNet => net.message.length()
    };
    n
}

// `+` panics on overflow but throws nothing: the channel is empty, yet
// the panic arm types by its claim.
function cf_shout(x: int) -> string throws never {
    let y = x + 1;
    if (y > 0) { "positive" } else { "non-positive" }
}

function cf_panic_catch(x: int) -> string throws never {
    let r = cf_shout(x) catch (e) {
     // ^ string
        let err: baml.panics.IntegerOverflow => err.message
    };
    r
}

// A wildcard arm over an EMPTY fact set lives in `never`, not the top
// type - the arm's `x` must not poison the join (regression: a
// throws-never base with `_ => x`).
function cf_empty_wildcard() -> int throws never {
    let n = {
     // ^ int
        let f = (x: int) -> int throws never { x };
        f(7)
    } catch (x) {
        _ => x
    };
    n
}
