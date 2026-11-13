// Interface calls compiled code dispatches on the receiver's class, and the
// receivers each site leaves to the VM.

interface DxShape {
    function area(self) -> int throws string
}

class DxSquare {
    side: int,
    implements DxShape {
        function area(self) -> int {
            if (self.side < 0) {
                throw `negative:${self.side}`;
            }
            self.side * self.side
        }
    }
}

class DxRect {
    w: int,
    h: int,
    implements DxShape {
        function area(self) -> int { (self.w * self.h) / self.h }
    }
}

// Pausing: the loop polls, so a live caller value is rooted across the call.
class DxSlow {
    n: int,
    implements DxShape {
        function area(self) -> int {
            let total = 0;
            for (let i = 0; i < self.n; i += 1) {
                total += await spawn { i % 3 };
            }
            total
        }
    }
}

// An extra optional parameter leaves this implementation to the VM.
class DxPadded {
    side: int,
    implements DxShape {
        function area(self, pad: int = 1) -> int { self.side + pad }
    }
}

class DxBox<T> {
    side: int,
    implements DxShape {
        function area(self) -> int { self.side * 10 }
    }
}

function DxArea(s: DxShape) -> int {
    s.area()
}

function DxTotal(shapes: DxShape[]) -> int {
    let total = 0;
    for (let s in shapes) {
        let before = total;
        total = before + s.area();
    }
    total
}

function DispatchKnown() -> void {
    let shapes: DxShape[] = [];
    for (let i = 0; i < 50; i += 1) {
        if (i % 2 == 0) {
            shapes.push(DxSquare { side: i });
        } else {
            shapes.push(DxRect { w: i, h: 2 });
        }
    }
    baml.io.println(`${DxTotal(shapes)}:${DxArea(DxRect { w: 7, h: 3 })}`);
}

function DispatchThrows() -> void {
    let thrown = DxArea(DxSquare { side: -2 }) catch (e) {
        let s: string => s,
    };
    let divided = DxArea(DxRect { w: 4, h: 0 }) catch (e) {
        baml.panics.DivisionByZero => -1,
    };
    baml.io.println(`${thrown}:${divided}`);
}

function DispatchSuspends() -> void {
    let shapes: DxShape[] = [DxSlow { n: 20 }, DxSquare { side: 3 }, DxSlow { n: 7 }];
    baml.io.println(`${DxTotal(shapes)}`);
}

function DispatchOptional() -> void {
    let shapes: DxShape[] = [DxPadded { side: 4 }, DxSquare { side: 4 }, DxPadded { side: 1 }];
    baml.io.println(`${DxTotal(shapes)}`);
}

function DispatchGeneric() -> void {
    let shapes: DxShape[] = [DxBox<int> { side: 2 }, DxRect { w: 5, h: 1 }, DxBox<string> { side: 3 }];
    baml.io.println(`${DxTotal(shapes)}`);
}

interface DxLabeled {
    label: string
    function shout(self) -> string throws never {
        self.label + "!"
    }
}

class DxLoud {
    label: string,
    implements DxLabeled {
        function shout(self) -> string throws never { self.label + "!!!" }
    }
}

class DxQuiet {
    label: string,
    implements DxLabeled {
        function shout(self) -> string throws never { self.label + "." }
    }
}

// Adopts the default body, whose frame binds `Self`.
class DxPlain {
    label: string,
    implements DxLabeled {}
}

function DxShout(value: unknown) -> string {
    match (value) {
        let labeled: DxLabeled => labeled.shout(),
        _ => "",
    }
}

function DispatchDefault() -> void {
    let out = [DxShout(DxLoud { label: "a" }), DxShout(DxPlain { label: "b" }), DxShout(DxQuiet { label: "c" })];
    baml.io.println(out.join(","));
}

function DispatchRuntimeClass() -> void {
    let minted = reflect.Class.new(
        "DxRow",
        { "label": string.type() },
        implementations = [reflect.Interface.implementation<DxLabeled>().field("label")],
    );
    let type Row = minted;
    let row: Row = baml.json.to<Row>({ "label": "r" });
    baml.io.println(`${DxShout(row)},${DxShout(DxLoud { label: "l" })},${DxShout(DxQuiet { label: "q" })}`);
}
