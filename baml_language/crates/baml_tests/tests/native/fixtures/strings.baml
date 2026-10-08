// Native backend fixture: strings built from scalars.

function greet(n: int) -> string {
    let s = "n=" + n.to_string();
    if (n % 2 == 0) {
        s = s + " even";
    } else {
        s = s + " odd";
    }
    s
}

function repeat_len(n: int) -> int {
    let s = "héllo";
    let out = "";
    let i = 0;
    while (i < n) {
        out = out + s;
        i = i + 1;
    }
    out.length()
}

function same(n: int) -> bool {
    let a = "x" + n.to_string();
    let b = "x" + (n + 0).to_string();
    a == b && !(a != b)
}

function ordered(n: int) -> bool {
    let a = n.to_string();
    let b = (n + 1).to_string();
    a < b
}

function ascii_check(n: int) -> bool {
    let s = "plain";
    if (n > 0) {
        s = s + "é";
    }
    s.is_ascii()
}
