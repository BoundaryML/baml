//
// (TIR leaves `Self` unrealized for members reached through the
// requires closure - `io.pull()` types `(Self as Sink).Item` - and
// then errors the `push` call against the T-flavored expectation.)
//
// I6: a `requires` target mentioning `Self` (`requires Source<Item =
// Self.Item>`) realizes against the SUBJECT of the elaboration -
// rustc's super-predicate instantiation. A member reached through the
// requires closure on a rigid `T extends Sink` types through the
// realized pin: `pull()` returns `(T as Source).Item`, which the
// elaborated pin equates with `(T as Sink).Item`, so the concrete
// receiver reduces it through the impl's binding.
interface Source {
    type Item

    function pull(self) -> Self.Item throws never
}

interface Sink requires Source<Item = Self.Item> {
    type Item

    function push(self, item: Self.Item) -> bool throws never
}

function roundtrip<T extends Sink>(io: T, c: bool) -> bool throws never {
    let v = io.pull();
     // ^ (T as user.Sink).Item
    if (c) { io.push(v) } else { false }
}

class Pipe {
    buffer int
    implements Source {
        type Item = int
        function pull(self) -> int throws never {
            self.buffer
        }
    }
    implements Sink {
        type Item = int
        function push(self, item: int) -> bool throws never {
            true
        }
    }
}

function concrete(p: Pipe) -> int throws never {
    let x = p.pull();
     // ^ int
    x
}
