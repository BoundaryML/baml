// State belongs to this BAML class. The interface exposes methods only.
interface Counter {
    function add(self, amount: int) -> int throws never
    function current(self) -> int throws never
}

class BamlCounter {
    stored_count: int,

    implements Counter {
        function add(self, amount: int) -> int throws never {
            self.stored_count = self.stored_count + amount;
            self.stored_count
        }
        function current(self) -> int throws never {
            self.stored_count
        }
    }
}

function MakeCounter() -> Counter throws never {
    BamlCounter { stored_count: 0 }
}

function AddInBaml(counter: Counter, amount: int) -> int throws never {
    counter.add(amount)
}

class Observation {
    initial: int,
    after_method: int,
    after_pass_back: int,
    through_old_alias: int,
    detached_read: int,
}

function observe_stateful_methods() -> Observation throws never {
    let counter = MakeCounter();
    let alias = counter;
    let initial = counter.current();
    let after_method = counter.add(2);
    let detached_read = counter.current();
    let after_pass_back = AddInBaml(counter, 3);
    Observation {
        initial,
        after_method,
        after_pass_back,
        through_old_alias: alias.current(),
        detached_read,
    }
}

test "methods mutate one BAML-owned receiver" {
    let observed = observe_stateful_methods();
    assert.equal(observed.initial, 0);
    assert.equal(observed.after_method, 2);
    assert.equal(observed.after_pass_back, 5);
    assert.equal(observed.through_old_alias, 5);
    assert.equal(observed.detached_read, 2)
}
