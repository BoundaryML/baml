// if-let scrutinees force occurring vars like match does
class IvDone {}
interface IvIterator<T, E> {
    function next(self) -> T | IvDone throws E

    function peekable(self) -> IvPeekable<T, E> throws never {
        IvPeekable<T, E> { iter: self, buffer: IvDone {}, has_buffer: false }
    }
}
class IvPeekable<T, E> {
    iter: IvIterator<T, E>
    buffer: T | IvDone
    has_buffer: bool

    function peek(self) -> T | IvDone throws E {
        if (self.has_buffer) {
            return self.buffer
        }
        let v = self.iter.next()
        self.buffer = v
        self.has_buffer = true
        v
    }

    implements IvIterator<T, E> {
        function next(self) -> T | IvDone throws E {
            if (self.has_buffer) {
                let v = self.buffer
                self.buffer = IvDone {}
                self.has_buffer = false
                return v
            }
            self.iter.next()
        }
    }
}
class IvArrayIterator<T> {
    arr: T[]
    idx: int

    function new(arr: T[]) -> IvArrayIterator<T> throws never {
        IvArrayIterator<T> { arr: arr, idx: 0 }
    }

    implements IvIterator<T, never> {
        function next(self) -> T | IvDone throws never {
            if (self.idx >= self.arr.length()) {
                return IvDone {}
            }
            let item = self.arr[self.idx]
            self.idx += 1
            item
        }
    }
}
function iv_main() -> int throws never {
    let p = IvArrayIterator.new([4, 5]).peekable()
    if let x: int = p.peek() {
        let r = x
         // ^ int
        r
    } else {
        0
    }
}
