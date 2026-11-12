function f(a: string, b: string) -> bool {
    !((a ?? b)).includes(`x`)
}
