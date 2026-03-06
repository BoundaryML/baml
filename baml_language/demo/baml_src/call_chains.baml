// ── Basic Function Calls ────────────────────────────────────────

function Identity(x: int) -> int {
    return x;
}

function CallOther() -> int {
    return Identity(42);
}

// ── Function Types ──────────────────────────────────────────────

function GetIdentity() -> (x: int) -> int {
    Identity
}

// ── Basic Call Chains ───────────────────────────────────────────

// f calls g calls h — return type flows back through chain
function Add1(x: int) -> int {
    return x + 1;
}

function Add2(x: int) -> int {
    return Add1(Add1(x));
}

function Add4(x: int) -> int {
    return Add2(Add2(x));
}

// chain with type change at each level
function IntToString(x: int) -> string {
    return "number";
}

function StringToLen(s: string) -> int {
    return 5;
}

// int → string → int
function RoundTrip(x: int) -> int {
    return StringToLen(IntToString(x));
}

// ── Return Type Mismatch Through Chains ─────────────────────────

// Add1 returns int, but we declare string → error
function ChainMismatch(x: int) -> string {
    return Add1(x);
}

// deeper: Add2 returns int, passing through Add1 twice
function DeepChainMismatch(x: int) -> string {
    return Add4(x);
}
