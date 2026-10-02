// A unit-returning body is an ordinary value position (BEP-079)
//
// Its tail is checked against the return type, never discarded: a value
// left there is a mismatch at the body, and the author ends it with `;`.
// Test bodies are synthesized `() -> void` lambdas, so the same holds
// for them. A statement is not observed: a statement-position `catch`
// yielding a value is fine.
function ub_log(log: string[]) -> void throws never {
    log.push("body");
}

function ub_tail(log: string[]) -> void throws never { log.push("body") }
//                                                   ^^^^^^^^^^^^^^^^^^^^ mismatch: expected null, got int

function ub_id(x: int) -> int throws never {
    let y = x;
     // ^ int
    y
}

test "ub_probe" {
    {
        let x = ub_id(2);
        match (x) {
            1 => "one",
            _ => { throw "boom" },
        }
    } catch (e) {
        string => null
    };
}
