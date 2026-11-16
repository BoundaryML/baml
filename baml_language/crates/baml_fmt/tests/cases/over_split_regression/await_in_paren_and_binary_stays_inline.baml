function main() -> string {
    let f = spawn { compute() };
    "got " + (await f)
}

function compute() -> string {
    "x"
}
