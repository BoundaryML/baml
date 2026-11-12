function f(a: string?, xs: string[]) -> int {
    (xs.at(a?.length ?? 0)).to_string().length()
}
