class Event {
    value: int,
}

type EventStream = Event[];

interface Live {
  function events(self) -> EventStream throws never
}

class Fixture {
    implements Live {
        function events(self) -> EventStream throws never {
            [Event { value: 2 }, Event { value: 3 }]
        }
    }
}

function total(live: Live) -> int throws never {
    let sum = 0;
    for (let event in live.events()) {
        sum += event.value
    }
    sum
}

function interface_method_array_alias_is_iterable_fn() -> int throws never {
    total(Fixture {  })
}

test "interface_method_array_alias_is_iterable" {
    assert.equal(interface_method_array_alias_is_iterable_fn(), 5)
}
