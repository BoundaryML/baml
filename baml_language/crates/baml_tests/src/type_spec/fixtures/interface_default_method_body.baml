//
// (TIR renders the frame slot as the projection `(Self as
// user.Store).Item` rather than the slot's own name.)
//
// I6: an interface DEFAULT-method body is an ordinary inference root.
// Inside it `self` is the rigid `Self` (frame slot 0) bounded by the
// interface AT ITS OWN FRAME (each associated slot pinned to the
// frame's own var), so a call through `self` resolves members via the
// carried bound exactly like `T extends I` would, and `Self.Item`
// reduces to the frame's `Item` slot - the same slot the runtime binds
// per-receiver at impl selection.
interface Store {
    type Item

    function get(self) -> Self.Item throws never

    function pair(self) -> Self.Item[] throws never {
        let a = self.get();
         // ^ Item
        [a, self.get()]
    }
}

class IntStore {
    value int
    implements Store {
        type Item = int
        function get(self) -> int throws never {
            self.value
        }
    }
}

function use_default(s: IntStore) -> int[] throws never {
    let r = s.pair();
     // ^ int[]
    r
}
