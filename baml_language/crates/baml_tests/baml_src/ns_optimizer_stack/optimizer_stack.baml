class Counter { value: int }

interface Source {
    function next(self) -> int throws never
}

class Sequence {
    counter: Counter
    implements Source {
        function next(self) -> int { tick(self.counter) }
    }
}

function tick(counter: Counter) -> int {
    counter.value = counter.value + 1
    counter.value
}

function number(value: int) -> int { value }
function combine(a: int, b: int) -> int { a * 10 + b }
function identity<T>(value: T) -> T { value }

function and_value(a: bool, b: bool, c: bool) -> bool { a && b && c }
function or_value(a: bool, b: bool, c: bool) -> bool { a || b || c }
function mixed_value(a: bool, b: bool, c: bool) -> bool { (a && b) || c }
function predicate(a: bool, b: bool, c: bool) -> bool {
    if (!(a && b) || c) { true } else { false }
}
function negated_truthiness(value: string?) -> int {
    if (!value) { 1 } else { 2 }
}
function mutable_value(a: bool, b: bool, c: bool) -> bool {
    let value = false
    if (c) { value = a && b }
    value
}

function truth_tables() -> bool {
    let bits = 0
    while (bits < 8) {
        let a = (bits & 1) != 0
        let b = (bits & 2) != 0
        let c = (bits & 4) != 0
        if (and_value(a, b, c) != (bits == 7)) { return false }
        if (or_value(a, b, c) != (bits != 0)) { return false }
        if (mixed_value(a, b, c) != ((bits & 3) == 3 || c)) { return false }
        if (predicate(a, b, c) != ((bits & 3) != 3 || c)) { return false }
        if (mutable_value(a, b, c) != (bits == 7)) { return false }
        bits = bits + 1
    }
    true
}

function condition_effects(a: bool, b: bool) -> int {
    let counter = Counter { value: 0 }
    if (a && { tick(counter); b } && { tick(counter); true }) { tick(counter) }
    counter.value
}

function coalesce(value: int?) -> int { value ?? 42 }
function coalesce_effect(value: int?) -> int {
    let counter = Counter { value: 0 }
    let result = value ?? tick(counter)
    result * 10 + counter.value
}
function coalesce_mutable(value: int?) -> int {
    let result = 8
    result = value ?? 42
    result
}

function array_mixed() -> int[] { [number(1), 2, number(3)] }
function array_effects() -> int[] {
    let counter = Counter { value: 0 };
    [tick(counter), 42, tick(counter), counter.value]
}
function failing_operand(counter: Counter, divisor: int) -> int {
    tick(counter)
    1 / divisor
}
function prefix_unwind(divisor: int) -> int {
    let counter = Counter { value: 0 }
    {
        let values = [tick(counter), 42, failing_operand(counter, divisor)]
        values[0] + values[1] + values[2]
    } catch (e) { baml.panics.DivisionByZero => counter.value }
}
function call_effects() -> int {
    let counter = Counter { value: 0 }
    combine(tick(counter), tick(counter))
}
function indirect_effects() -> int {
    let counter = Counter { value: 0 }
    let call = (a: int, b: int) => { a * 10 + b }
    call(tick(counter), tick(counter))
}
function binary_effects() -> int {
    let counter = Counter { value: 0 }
    tick(counter) - tick(counter)
}
function virtual_result(source: Source) -> int { source.next() }
function virtual_effects() -> int[] {
    let source: Source = Sequence { counter: Counter { value: 0 } };
    [virtual_result(source), 42, source.next()]
}
function generic_effects() -> int {
    let counter = Counter { value: 0 }
    combine(identity<int>(tick(counter)), identity<int>(tick(counter)))
}

function constants() -> int { let a = 1 + 2; let b = a * 3; b + 4 }
function overwritten(a: bool) -> int {
    let result = 0
    if (a) { result = 1 } else { result = 2 }
    result
}
function overwritten_panic(divisor: int) -> int {
    {
        let result = 1 / divisor
        result = 42
        result
    } catch (e) { baml.panics.DivisionByZero => 7 }
}
function handler_observes_store(divisor: int) -> int {
    let result = 1
    {
        result = 2
        let unused = 1 / divisor
        result = 3
        result
    } catch (e) { baml.panics.DivisionByZero => result }
}
function constant_overflow() -> int {
    {
        let largest = 4611686018427387903
        let one = 1
        largest + one
    } catch (e) { baml.panics.IntegerOverflow => 7 }
}
function parameter_copy(value: int) -> int {
    let before = value
    value = 8
    before
}
function loop_values(count: int) -> int {
    let index = 0
    let total = 0
    while (index < count && total < 20000) {
        let result = (index < 500 && index < 250) || index >= 750
        if (result) { total = total + 1 }
        index = index + 1
    }
    total
}

function iterator_sum(values: int[]) -> int {
    let sum = 0
    for (let value in values) { sum = sum + value }
    sum
}

function verify_all() -> bool {
    truth_tables()
        && negated_truthiness("hi") == 2
        && negated_truthiness("") == 1 && negated_truthiness(null) == 1
        && condition_effects(false, true) == 0
        && condition_effects(true, false) == 1
        && condition_effects(true, true) == 3
        && coalesce(null) == 42 && coalesce(0) == 0
        && coalesce_effect(null) == 11 && coalesce_effect(0) == 0
        && coalesce_mutable(null) == 42 && coalesce_mutable(9) == 9
        && array_mixed() == [1, 2, 3]
        && array_effects() == [1, 42, 2, 2]
        && prefix_unwind(0) == 2 && prefix_unwind(1) == 44
        && call_effects() == 12 && indirect_effects() == 12
        && binary_effects() == -1 && virtual_effects() == [1, 42, 2]
        && generic_effects() == 12 && constants() == 13
        && overwritten(true) == 1 && overwritten(false) == 2
        && overwritten_panic(0) == 7 && overwritten_panic(1) == 42
        && handler_observes_store(0) == 2 && handler_observes_store(1) == 3
        && constant_overflow() == 7 && parameter_copy(3) == 3
        && loop_values(1000) == 500
        && iterator_sum([1, 2, 3, 4]) == 10 && iterator_sum([]) == 0
}

test "optimized value and predicate paths preserve behavior" {
    assert.is_true(verify_all())
}
