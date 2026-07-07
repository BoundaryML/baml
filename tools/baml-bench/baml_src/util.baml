// Small deterministic helpers shared across stages.

// int -> string (no int.to_string on this pin; unstable.string is the fleet idiom).
function i2s(n: int) -> string {
    baml.unstable.string(n)
}

// Truncate to at most n characters (codepoints), appending a marker when cut.
function truncate(s: string, n: int) -> string {
    if (s.length() <= n) {
        s
    } else {
        s.substring(0, n) + "\n…[truncated]"
    }
}

// Collapse all whitespace runs to single spaces (used to compare an evidence
// quote against chunk source without caring about indentation/newlines).
function squash_ws(s: string) -> string {
    let out = s.replace_all("\t", " ").replace_all("\r", " ").replace_all("\n", " ");
    while (out.includes("  ")) {
        out = out.replace_all("  ", " ");
    }
    out.trim()
}

// True when `needle` occurs in `haystack` after whitespace normalization.
// Deterministic gate for LLM-provided evidence quotes.
function evidence_in_source(evidence: string, source: string) -> bool {
    let e = squash_ws(evidence);
    if (e.length() == 0) {
        return false;
    }
    squash_ws(source).includes(e)
}

// Is `ch` (a 1-char string) part of an identifier?
function is_ident_char(ch: string) -> bool {
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_".includes(ch) && ch != ""
}

// All identifiers appearing in `source`, deduped, sorted.
function identifiers(source: string) -> string[] {
    let seen: map<string, bool> = {};
    let cur = "";
    for (let ch in source.split("")) {
        if (is_ident_char(ch)) {
            cur = cur + ch;
        } else {
            if (cur.length() > 0) {
                seen.set(cur, true);
            }
            cur = "";
        }
    }
    if (cur.length() > 0) {
        seen.set(cur, true);
    }
    seen.keys()
        .sort_by_key(
            (k: string) -> string {
                k
            },
        )
}

// Render a float with 2 decimals (no format mini-language on this toolchain).
function f2(x: float) -> string {
    let rounded = (x * 100.0).iround() catch (_) {
        _ => 0
    };
    let whole = rounded / 100;
    let frac = rounded - whole * 100;
    let fa = if (frac < 0) {
        0 - frac
    } else {
        frac
    };
    let fs = if (fa < 10) {
        "0" + i2s(fa)
    } else {
        i2s(fa)
    };
    i2s(whole) + "." + fs
}

// The anchored rubric's grade bands, as the single source of verdict truth.
function verdict_of(grade: int) -> string {
    if (grade >= 7) {
        "adherent"
    } else {
        if (grade >= 5) {
            "neutral"
        } else {
            if (grade >= 3) {
                "fighting"
            } else {
                "reinventing"
            }
        }
    }
}
