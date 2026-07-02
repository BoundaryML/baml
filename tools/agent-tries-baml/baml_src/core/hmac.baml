// HMAC-SHA256 via openssl — BAML has no crypto stdlib, so webhook signature
// verification (Linear-Signature, Slack v0) shells out. openssl is short-lived
// with no grandchildren, so plain sys.exec (pipe capture) is safe here — no
// need for run_capture's file-redirect wrapper. The message goes on STDIN;
// the key rides argv, which stays inside the machine boundary (same trust
// domain as the env var it came from).

// Hex HMAC-SHA256 of `message` keyed by `secret`; "" when openssl is missing
// or errors (callers treat "" as verification failure, never as a match).
function hmac_sha256_hex(secret: string, message: string) -> string {
    let out = baml
        .sys
        .exec(
            "openssl",
            ["dgst", "-sha256", "-hmac", secret, "-r"],
            baml.sys.ProcessOptions { cwd: "/tmp", env: null, timeout_ms: 10000, stdin: message },
        ) catch (_) {
        _ => {
            return "";
        }
    };
    if (out.exit_code != 0) {
        return "";
    }
    let text = out.stdout.to_string() catch (_) {
        _ => ""
    };
    // `-r` prints "<hex> *stdin"; take the hex token.
    return match (text.trim().split(" ").at(0)) {
        let h: string => h.trim(),
        null => "",
    };
}

// Constant-time-ish string equality for signature comparison. BAML has no
// compare_digest; length-gate + full-scan XOR-style accumulation avoids the
// obvious early-exit timing channel of `==` on secrets.
function digest_eq(a: string, b: string) -> bool {
    if (a.length() != b.length() || a.length() == 0) {
        return false;
    }
    let av = a.split("");
    let bv = b.split("");
    let diff = 0;
    let i = 0;
    while (i < av.length()) {
        let ca = match (av.at(i)) {
            let v: string => v,
            null => "",
        };
        let cb = match (bv.at(i)) {
            let v: string => v,
            null => "",
        };
        if (ca != cb) {
            diff = diff + 1;
        }
        i = i + 1;
    }
    return diff == 0;
}
