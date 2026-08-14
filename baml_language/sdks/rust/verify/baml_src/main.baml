// Minimal expression functions (no LLM) so the verifier exercises the full
// call path — encode -> C ABI -> engine -> decode -> both sync and async
// entry points — without network or credentials.

function rt_int(x: int) -> int { x }

function rt_greet(name: string) -> string { "hi" }
