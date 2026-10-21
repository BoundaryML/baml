// match scrutinees force their occurring vars at the exhaustiveness demand
class PkDone {}
interface PkIterable<T, E> {
    function iter(self) -> PkIterator<T, E> throws never
}
interface PkIterator<T, E> requires PkIterable<T, E> {
    function next(self) -> T | PkDone throws E

    function peekable(self) -> PkPeekable<T, E> throws never {
        PkPeekable<T, E> { iter: self, buffer: PkDone {}, has_buffer: false }
    }
}
class PkPeekable<T, E> {
    iter: PkIterator<T, E>
    buffer: T | PkDone
    has_buffer: bool

    function peek(self) -> T | PkDone throws E {
        if (self.has_buffer) {
            return self.buffer
        }
        let v = self.iter.next()
        self.buffer = v
        self.has_buffer = true
        v
    }

    implements PkIterable<T, E> {
        function iter(self) -> PkIterator<T, E> throws never { self }
    }

    implements PkIterator<T, E> {
        function next(self) -> T | PkDone throws E {
            if (self.has_buffer) {
                let v = self.buffer
                self.buffer = PkDone {}
                self.has_buffer = false
                return v
            }
            self.iter.next()
        }
    }
}
class PkArrayIterator<T> {
    arr: T[]
    idx: int

    function new(arr: T[]) -> PkArrayIterator<T> throws never {
        PkArrayIterator<T> { arr: arr, idx: 0 }
    }

    implements PkIterable<T, never> {
        function iter(self) -> PkIterator<T, never> throws never { self }
    }

    implements PkIterator<T, never> {
        function next(self) -> T | PkDone throws never {
            if (self.idx >= self.arr.length()) {
                return PkDone {}
            }
            let item = self.arr[self.idx]
            self.idx += 1
            item
        }
    }
}
function pk_main() -> int[] {
    let p = PkArrayIterator.new([4, 5]).peekable()
     // ^ user.PkPeekable<int, never>
    let out: int[] = []
    match (p.peek()) {
        PkDone => {},
        let x: int => out.push(x),
    }
    out
}
