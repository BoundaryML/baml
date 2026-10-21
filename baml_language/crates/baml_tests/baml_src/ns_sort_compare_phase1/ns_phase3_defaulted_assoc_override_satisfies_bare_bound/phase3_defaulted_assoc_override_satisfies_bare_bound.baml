interface CmpD {
    type CE = never;
    function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE;
}

interface SrtD {
    type SE;
    function srt(self) -> Self throws Self.SE;
}

implements<T extends CmpD> SrtD for T[] {
            type SE = T.CE
            function srt(self) -> Self throws T.CE {
                self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
            }
        }

class BoomD {
    message: string,
}

class WithErrD {
    v: int,
    implements CmpD {
        type CE = BoomD;
        function comp(self, other: Self) -> baml.ops.Ordering throws BoomD {
            return baml.ops.Ordering.Equal;
        }
    }
}

function f(xs: WithErrD[]) -> WithErrD[] throws BoomD {
    xs.srt()
}

test "phase3_defaulted_assoc_override_satisfies_bare_bound" {
    assert.equal(1, 1)
}
