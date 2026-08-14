// `Self` in written types inside implements blocks
//
// r-a's resolver carries the impl's self type: `Self` in any written
// position (params, returns, nested constructors) resolves to the
// IMPLEMENTOR inside class-owned and free-impl methods. Interface
// owners keep the universal frame slot - there `Self` is a type
// variable, not a concrete type.
class SisScore {
    points int

    implements baml.Comparable {
        type CompareError = never

        function compare(self, other: Self) -> int throws never {
            let p = other.points;
             // ^ int
            self.points.compare(p)
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

function sis_use(a: SisScore, b: SisScore) -> int throws never {
    a.compare(b)
}
