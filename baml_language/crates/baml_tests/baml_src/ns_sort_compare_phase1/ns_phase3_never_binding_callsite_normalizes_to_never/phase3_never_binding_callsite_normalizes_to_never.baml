interface Cmp2 {
    type CE;
    function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE;
}

interface Srt2 {
    type SE;
    function srt(self) -> Self throws Self.SE;
}

implements<T extends Cmp2> Srt2 for T[] {
        type SE = T.CE

        function srt(self) -> Self throws T.CE {
            self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
        }
    }

class Boom2 {
    message: string,
}

class WithNever {
    v: int,
    implements Cmp2 {
        type CE = never;
        function comp(self, other: Self) -> baml.ops.Ordering throws never {
            return baml.ops.Ordering.Equal;
        }
    }
}

class WithErr {
    v: int,
    implements Cmp2 {
        type CE = Boom2;
        function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 {
            return baml.ops.Ordering.Equal;
        }
    }
}

function f(xs: WithNever[]) -> WithNever[] throws never {
    xs.srt()
}

test "phase3_never_binding_callsite_normalizes_to_never" {
    assert.equal(1, 1)
}
