function leaf(n: int) -> int { n }
function main(n: int) -> int { let s = 0; for (let i = 0; i < n; i += 1) { s += leaf(1); }; s }
