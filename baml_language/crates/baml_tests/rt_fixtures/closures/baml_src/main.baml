class UnallocatedArgument { value: int }
interface Adder { function add(self, n: int) -> int throws never; }
class Counter {
    value: int,
    implements Adder {
        function add(self, n: int) -> int throws never { self.value += n; self.value }
    }
}
function bind(value: Adder) -> (int) -> int throws never { value.add }
class Boxed { callback: (int) -> int throws never }
function increment(n: int) -> int { n + 1 }
function negate(value: bool) -> bool { !value }
function invoke(f: (int) -> int throws never, n: int) -> int { f(n) }
function make(n: int) -> (int) -> int throws never {
    let shared = n;
    (x: int) -> int { shared += x; shared }
}
function empty() -> int {
    let fns: ((UnallocatedArgument) -> int throws never)[] = [];
    fns.length()
}
function main() -> int {
    let unused: ((UnallocatedArgument) -> int throws never)[] = [];
    let _ = unused.length();
    let count = 1;
    let outer = () -> ((int) -> int throws never) throws never {
        (n: int) -> int { count += n; count }
    };
    let f = outer();
    let recursive: (int) -> int throws never = (n: int) -> int { n };
    recursive = (n: int) -> int {
        if (n == 0) { count } else { recursive(n - 1) + 1 }
    };
    let stored = Boxed { callback: f };
    let callbacks: ((int) -> int throws never)[] = [increment, stored.callback, make(2)];
    let predicate = negate;
    let delta = if (predicate(false)) { 0 } else { 100 };
    let text = "abc";
    let length = () -> int { text.length() };
    let counter = Counter { value: 3 };
    let method = counter.add;
    let virtual_method = bind(counter);
    let each: (() -> int throws never)[] = [];
    for (let i = 0; i < 3; i += 1) {
        let n = i;
        each.push(() -> int { n });
    }
    invoke(callbacks[0], recursive(2)) + callbacks[1](3) + callbacks[2](4) + method(5) + each[0]() + each[1]() + each[2]() + length() + virtual_method(2) + delta
}

function RtClosures() -> int { main() }
function RtEmptyClosures() -> int { empty() }
