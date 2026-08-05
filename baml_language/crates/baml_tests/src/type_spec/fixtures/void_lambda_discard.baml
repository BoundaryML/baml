// Void LAMBDA bodies discard their tail value (test bodies are
// synthesized `() -> void` lambdas) - the statement semantics void
// functions already get; a statement-position catch yielding a value
// is fine.
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
    }
}
