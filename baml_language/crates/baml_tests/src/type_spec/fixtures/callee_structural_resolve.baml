// callees structurally resolve before the function-shape demand
function fgp_deep_copy() -> int throws never {
    let x = 0
    let inc = () -> int { x += 1; x }
    let inc2 = baml.deep_copy(inc)
     // ^^^^ () -> int throws never
    inc()
    let n = inc2()
     // ^ int
    n
}
function fgp_push_only() -> int throws never {
    let sum = 0
    let total = []
    for (let i in [1, 2, 3]) {
        total.push(() -> void { sum += i })
    }
    for (let cb in total) {
        cb()
    }
    let done = sum
     // ^^^^ int
    done
}
