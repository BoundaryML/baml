// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
// docs:endmeta
interface PriorityComparison {
    type CompareError
    function compare(self, other: Self) -> int throws Self.CompareError
}

class DeliveryPriority {
    rank: int,

    implements PriorityComparison {
        function compare(self, other: Self) -> int throws never {
            if (self.rank < other.rank) { -1 } else if (self.rank > other.rank) { 1 } else { 0 }
        }
    }
}
