// weak aliases resolve at every structure demand
// (aliased Future await: TIR records a false mismatch and types unknown)
type AsdMaybe = string | null
type AsdHandler = (int) -> string
type AsdInt = int
type AsdNums = int | float
type AsdJob = baml.future.Future<int, never>
function asd_chain(m: AsdMaybe) -> int throws never {
    let n = m?.length() ?? 0
     // ^ int
    n
}
function asd_lambda() -> string throws never {
    let f: AsdHandler = (x) -> { "v" }
    let r = f(1)
     // ^ string
    r
}
function asd_operator(a: AsdInt, b: AsdInt) -> int throws never {
    let r = a + b
     // ^ int
    r
}
function asd_union_operand(a: AsdNums) -> int | float throws never {
    let r = a + a
     // ^ int | float
    r
}
function asd_await(j: AsdJob) -> int throws never {
    let v = await j
     // ^ int
    v
}
