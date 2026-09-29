// Void LAMBDA bodies discard their tail value - the statement semantics void
// functions already get; a statement-position catch yielding a value is
// fine. (`test` bodies are the exception: they must themselves be void,
// E0176, so this probe is a plain lambda.)
function ptc_id(x: int) -> int throws never {
    let y = x;
     // ^ int
    y
}

function ptc_probe() -> void {
    let probe = () -> void {
        {
            let x = ptc_id(2);
            match (x) {
                1 => "one",
                _ => { throw "boom" },
            }
        } catch (e) {
            string => null
        }
    };
    probe()
}
