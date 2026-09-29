// Void LAMBDA bodies must not evaluate to a value (E0176), like void
// functions; test bodies are synthesized `() -> void` lambdas. A
// statement-position catch yielding a value is fine: the `;` after it
// discards the value.
function ptc_id(x: int) -> int throws never {
    let y = x;
     // ^ int
    y
}

test "ptc_probe" {
    {
        let x = ptc_id(2);
        match (x) {
            1 => "one",
            _ => { throw "boom" },
        }
    } catch (e) {
        string => null
    };
}
