function GlobalAdd(value: int, by: int = 2) -> int {
    value + by
}

const GLOBAL_FUNCTION = GlobalAdd;
const GLOBAL_CLOSURE = {
    let state = [7];
    (value: int) -> int {
        state[0] += value;
        state[0]
    }
};

class GlobalMultiplier {
    by: int,
    function multiply(self, value: int) -> int { self.by * value }
}

const GLOBAL_METHOD = GlobalMultiplier { by: 3 }.multiply;
const GLOBAL_ALIAS = GLOBAL_CLOSURE;
const GLOBAL_THROWING = (value: int) -> int { throw `global-call:${value}` };

function GlobalCalls() -> void {
    baml.io.println(`${GLOBAL_FUNCTION(4)}:${GLOBAL_FUNCTION(4, by = 5)}`);
    baml.io.println(`${GLOBAL_CLOSURE(1)}:${GLOBAL_ALIAS(2)}:${GLOBAL_METHOD(5)}`);
    let allocated = ["kept across a global closure call"];
    baml.io.println(`${GLOBAL_CLOSURE(3)}:${allocated[0]}`);
    baml.io.println(`${GLOBAL_CLOSURE(1, $trace = trace.span(inputs = true, output = true))}`);
    let caught = GLOBAL_THROWING(9) catch (error, context) {
        _ => `${error}:${context.error}:${context.stack_trace.frames.length()}`,
    };
    baml.io.println(`${caught}`);
}

function GlobalCallPending() -> void {
    baml.io.println("unreachable: global callback was not initialized");
}
