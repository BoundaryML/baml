function replace(t: (int, string)) -> null { t.0 = 2; }
function compound(t: (int, string)) -> null { t.0 += 2; }
function nested(t: ((int,), string)) -> null { t.0.0 = 2; }
function optional(t: (int, string)?) -> null { t?.0 = 2; }
function bracket(t: (int, string)) -> int { t[0] }
function union_first(t: (int, string) | (float, string)) -> unknown { t.0 }
function union_common(t: (int, string) | (float, string)) -> string { t.1 }
function optional_union(t: (int, string) | (float, string) | null) -> string? { t?.1 }
