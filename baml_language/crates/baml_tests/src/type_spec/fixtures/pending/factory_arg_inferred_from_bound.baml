// pending: symbolic impl resolution (obligation-driven impl inversion)
//
// tir: fails
//
// (The RESULT type already resolves in hir_ty - `r: int` via return
// backprop pinning `O`, effects defaulting `E` - where TIR gives
// `unknown`. What stays unsolved is the FACTORY's own argument:
// `generate()` erases to `Generate<!error>` because discharging
// `Implements(Generate<?F>, Driver<Task<int>, ...>)` requires
// inverting the blanket impl. The caret pins the ticket's acceptance
// criterion: `Generate<int>`.)
//
// B-898 (https://linear.app/boundaryml2/issue/B-898): a zero-argument
// generic factory passed to a generic method must be constrained
// outside-in: `D := Generate<?F>` meets the bound
// `D extends Driver<Task<int>, ...>`, and discharging that obligation
// requires INVERTING the blanket impl (match `Generate<?F>` against
// its for-pattern to derive `?F = int`). The obligation machinery (I4)
// registers and stalls correctly; the missing piece is the symbolic
// impl resolver the registry defers ("candidates whose implemented
// head still carries impl variables are skipped"). Expected once impl
// inversion lands.
interface Driver<Input> {
    type Output
    type Error

    function drive(self, input: Input) -> Self.Output throws Self.Error
}

class Task<T> {
    value T

    function drive<O, E, D extends Driver<Task<T>, Output = O, Error = E>>(
        self,
        driver: D,
    ) -> O throws E {
        driver.drive(self)
    }
}

class Generate<T> {
    tag string
}

implement<T> Driver<Task<T>> for Generate<T> {
    type Output = T
    type Error = never

    function drive(self, input: Task<T>) -> T throws never {
        input.value
    }
}

function generate<T>() -> Generate<T> {
    Generate<T> { tag: "g" }
}

function repro(task: Task<int>) -> int throws never {
    let r = task.drive(generate());
             //        ^^^^^^^^^^ user.Generate<int>
    r
}
