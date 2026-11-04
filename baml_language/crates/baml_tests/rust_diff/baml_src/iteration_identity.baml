class IterationIdentityValue {
    value int
}

interface Launchable {
    function iter(self) -> IterationIdentityValue[] throws never;
    function next(self) -> int throws never;
}

implements Launchable for IterationIdentityValue[] {
    function iter(self) -> IterationIdentityValue[] throws never {
        self.push(IterationIdentityValue { value: 10 });
        self
    }
    function next(self) -> int throws never {
        self.push(IterationIdentityValue { value: 20 });
        self.length()
    }
}

function IteratorInterfaceIdentity() -> void {
    let xs = [IterationIdentityValue { value: 1 }];
    let it = Launchable.iter(xs);
    let count = Launchable.next(it);
    baml.io.println(`${count}:${xs.length()}:${xs[1].value}:${xs[2].value}`);

    let numbers = [1, 10, 20];
    let total = 0;
    for (let x in numbers) { total += x; }
    let explicit = numbers.iter();
    let first = explicit.next();
    baml.io.println(`${total}:${first}`);
}
