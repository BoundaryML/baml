// Native backend fixture: methods on concrete classes.

class Counter {
    n: int
    function get(self) -> int { self.n }
    function add(self, by: int) -> int {
        self.n = self.n + by;
        self.n
    }
}

function count(k: int) -> int {
    let c = Counter { n: 0 };
    let i = 0;
    while (i < k) {
        c.add(i);
        i = i + 1;
    }
    c.get()
}

function alias(k: int) -> int {
    let a = Counter { n: k };
    let b = a;
    b.add(1);
    a.get()
}
