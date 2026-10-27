type F = (x: int) -> int throws never;

class Holder {
    f: F?,
}

class G {
    function wrong_argument(self, x: string) -> int {
        0
    }
    function wrong_result(self, x: int) -> string {
        "wrong"
    }
    function throwing(self, x: int) -> int throws string {
        throw "error"
    }
}

function field_argument_mismatch() -> void {
    let h = Holder { f: null };
    let g = G {};
    h.f = g.wrong_argument;
}

function map_result_mismatch() -> void {
    let m: map<string, F> = {};
    let g = G {};
    m["a"] = g.wrong_result;
}

function field_throws_mismatch() -> void {
    let h = Holder { f: null };
    let g = G {};
    h.f = g.throwing;
}

function map_throws_mismatch() -> void {
    let m: map<string, F> = {};
    let g = G {};
    m["a"] = g.throwing;
}
