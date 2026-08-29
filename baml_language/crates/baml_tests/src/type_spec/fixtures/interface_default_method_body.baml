// I6: an interface DEFAULT-method body is an ordinary inference root.
// Inside it `self` is the rigid `Self` (frame slot 0) bounded by the
// interface, so a call through `self` resolves members via the carried
// bound exactly like `T extends I` would. Associated types are not
// frame slots: `Self.Item` is the rigid projection
// `(Self as user.Store).Item` - the canonical `(Self, interface, name)`
// triple - which the runtime reduces per-receiver through the selected
// impl's bindings.
interface Store {
    type Item

    function get(self) -> Self.Item throws never

    function pair(self) -> Self.Item[] throws never {
        let a = self.get();
         // ^ (Self as user.Store).Item
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
