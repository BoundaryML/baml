// to_string operator-style fallback (string.from)
//
// `recv.to_string()` with no real `implements baml.ToString` member is
// language sugar for `string.from(recv)` - total, `throws never`,
// honoring overrides via the runtime shim. Same lang-item discipline
// as the `to_json` desugar; a real ToString impl still resolves first.
class TsfPlain {
    v int
}

function big_str() -> string throws never {
    let s = 42n.to_string();
     // ^ string
    s
}

function float_str(f: float) -> string throws never {
    let t = (-0.5).to_string();
     // ^ string
    f.to_string()
}

function box_str(b: TsfPlain) -> string throws never {
    b.to_string()
}
