function f(name: string) -> int {
    let future = spawn // after spawn
        name /* before with */ with /* before first */ first(), /* after comma */ second() /* before body */ { 1 };
    await future
}
