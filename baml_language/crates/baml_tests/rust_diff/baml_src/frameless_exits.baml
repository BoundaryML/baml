// Pending records linked on the way out of compiled code: frameless bodies
// that dispatch on an interface (directly or through the VM's fallback),
// enter an interface default with `Self`, await, log, or throw. Traces,
// contexts and output must look as they do with a record per call.

interface FxCounter {
    base: int
    function step(self) -> int throws string
    function total(self) -> int throws string {
        self.base + self.step()
    }
}

class FxInc {
    base: int,
    implements FxCounter {
        function step(self) -> int throws string { 1 }
    }
}

class FxBad {
    base: int,
    implements FxCounter {
        function step(self) -> int throws string {
            if (self.base < 0) {
                throw `bad:${self.base}`;
            }
            self.base
        }
    }
}

function FxRun(c: FxCounter) -> int {
    c.total()
}

function FxSum(n: int) -> int {
    let acc = 0;
    for (let i = 0; i < n; i += 1) {
        if (i % 2 == 0) {
            acc += FxRun(FxInc { base: i });
        } else {
            acc += FxRun(FxBad { base: i });
        }
    }
    acc
}

// A throw three frameless levels below its handler, through a default body
// entered with `Self` and an interface arm.
function FxThrows() -> string {
    let r = FxRun(FxBad { base: -2 }) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    `${r}`
}

// A runtime class takes the VM's dispatch fallback from inside a pending
// body; a static class takes its arm.
interface FxNamed {
    label: string
    function shout(self) -> string throws never {
        self.label + "!"
    }
}

class FxLoud {
    label: string,
    implements FxNamed {}
}

function FxShout(value: unknown) -> string {
    match (value) {
        let named: FxNamed => named.shout(),
        _ => "",
    }
}

function FxRuntime() -> string {
    let minted = reflect.Class.new(
        "FxRow",
        { "label": string.type() },
        implementations = [reflect.Interface.implementation<FxNamed>().field("label")],
    );
    let type Row = minted;
    let row: Row = baml.json.to<Row>({ "label": "r" });
    `${FxShout(row)},${FxShout(FxLoud { label: "l" })}`
}

function FxAwaits(n: int) -> int {
    let total = 0;
    for (let i = 0; i < n; i += 1) {
        total += await spawn { i % 3 };
    }
    total
}

function FxLogs(n: int) -> int {
    log.info("frameless log");
    n + 1
}

function FxAwaitThrows(n: int) -> int {
    let v = await spawn { n };
    if (v == 4) {
        throw `after-await:${v}`;
    }
    v
}

function FxDeep(n: int) -> int {
    FxAwaitThrows(n) + 1
}

// Sibling pending bodies with an interface call at the same pc, on a receiver
// the VM's cached fallback dispatches: each must key the site cache on itself,
// not on the caller they share as the nearest linked record.
interface FxPair {
    function first(self) -> int throws never
    function second(self) -> int throws never
}

// Extra optional parameters leave both methods to the VM.
class FxPadded {
    n: int,
    implements FxPair {
        function first(self, pad: int = 0) -> int throws never { self.n + pad }
        function second(self, pad: int = 0) -> int throws never { self.n * 100 + pad }
    }
}

function FxFirst(p: FxPair) -> int { p.first() }

function FxSecond(p: FxPair) -> int { p.second() }

function FramelessExits() -> void {
    let pair: FxPair = FxPadded { n: 2 };
    baml.io.println(`${FxFirst(pair)} ${FxSecond(pair)}`);
    baml.io.println(`${FxSum(40)}`);
    baml.io.println(FxThrows());
    baml.io.println(FxRuntime());
    baml.io.println(`${FxAwaits(9)}:${FxLogs(2)}`);
    let r = FxDeep(4) catch (e, ctx) {
        _ => `${ctx.error}|${Frames(ctx)}`,
    };
    baml.io.println(`${r}`);
}
