// `Self` in written types inside implements blocks
//
// r-a's resolver carries the impl's self type: `Self` in any written
// position (params, returns, nested constructors) resolves to the
// IMPLEMENTOR inside class-owned and free-impl methods. Interface
// owners keep the universal frame slot - there `Self` is a type
// variable, not a concrete type.
class SisScore {
    points int

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.points == other.points
        }
    }

    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            let p = other.points;
             // ^ int
            self.points.cmp(p)
        }
    }
}

class SisPair {
    left int

    function twin(self, mate: Self) -> Self throws never {
        let m = mate.left;
         // ^ int
        self
    }
}

function sis_use(a: SisScore, b: SisScore) -> baml.ops.Ordering throws never {
    a.cmp(b)
}
