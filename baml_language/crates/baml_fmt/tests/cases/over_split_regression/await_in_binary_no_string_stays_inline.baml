function main() -> int {
    let f = spawn { 1 };
    1 + (await f)
}
