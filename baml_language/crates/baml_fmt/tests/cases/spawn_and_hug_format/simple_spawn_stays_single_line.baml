function f() -> int {
    let nm = "n";
    let a = spawn nm { 7 };
    let b = spawn nm with baml.spawn.Root.new() { 8 };
    (await a) + (await b)
}
