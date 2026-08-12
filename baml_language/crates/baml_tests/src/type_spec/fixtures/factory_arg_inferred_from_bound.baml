//
// B-898 (https://linear.app/boundaryml2/issue/B-898): a zero-argument
// generic factory passed to a generic method is constrained
// OUTSIDE-IN. The chain: the argument aliases the method's `D` var
// (generalization - a var whose only bound is one var-carrying lower
// is union-find aliasing, rustc's combine shape), the bound
// `D extends Driver<Task<int>, ...>` becomes an Implements obligation
// with a concrete head, and SELECTION confirms the unique blanket impl
// (fresh vars for the impl params, header unification commits) -
// deriving `?F = int` and pinning Output/Error through the impl's
// bindings. rustc's unique-candidate selection, licensed by I7
// coherence. TIR: `Generate<unknown>` plus spurious errors.
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
