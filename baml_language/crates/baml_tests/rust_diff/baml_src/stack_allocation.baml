// Standalone as well as part of the shared differential project: its rt pack
// must exercise placement even while unrelated project initializers are unsupported.
class StackLeaf {
    value: int
    function read(self) -> int { self.value }
}
class StackHolder {
    leaf: StackLeaf
    text: string
}

function StackMake(value: int) -> StackLeaf { StackLeaf { value: value } }
function StackBorrow(value: StackLeaf) -> int { value.read() }
function StackAlias(value: StackLeaf) -> StackLeaf { value }
function StackStore(target: StackHolder, value: StackLeaf) -> int {
    target.leaf = value;
    0
}

function StackReuse() -> int {
    let previous = StackLeaf { value: 0 };
    let total = 0;
    for (let i = 1; i < 100; i += 1) {
        let current = StackLeaf { value: i };
        total += StackBorrow(previous);
        previous = current;
    }
    total + previous.value
}

function StackIndirectReuse() -> int {
    let holder = StackHolder { leaf: StackMake(0), text: "holder" };
    let total = 0;
    for (let i = 1; i < 100; i += 1) {
        let current = StackLeaf { value: i };
        total += holder.leaf.value;
        StackStore(holder, current);
    }
    total + holder.leaf.value
}

class StackFault { code: int }
function StackMaybe(value: StackLeaf, fail: bool) -> StackLeaf throws StackFault {
    if (fail) { throw StackFault { code: 1 }; }
    value
}
function StackExceptionReuse() -> int {
    let previous = StackMake(3);
    let total = 0;
    for (let i = 0; i < 20; i += 1) {
        let current = StackLeaf { value: i };
        // The old previous value must survive when the call throws.
        previous = StackMaybe(current, i % 2 == 0) catch (e) {
            StackFault => previous
        };
        total += previous.value;
    }
    total
}

function StackAllocation() -> int {
    let local = StackLeaf { value: 7 };
    let alias = StackAlias(local);
    let borrowed = StackBorrow(alias);
    let lent = StackLeaf { value: 13 };
    // A function-local heap list may borrow this stack object until return.
    let borrowers: StackLeaf?[] = [lent];
    // Both fields hold heap pointers. They must survive every collection
    // through the conservative scan of this stack object's typed layout.
    let holder = StackHolder { leaf: StackMake(41), text: "a" + "b" };
    let sum = 0;
    for (let i = 0; i < 100; i += 1) {
        let garbage = StackMake(i);
        sum += garbage.value;
    }
    let loan = borrowers[0];
    let loan_value = if (loan != null) { loan.value } else { 0 };
    borrowed + loan_value + holder.leaf.value + holder.text.length() + sum + StackReuse() + StackIndirectReuse() + StackExceptionReuse()
}
