// Content-addressed JSON cache — the pipeline's determinism backbone.
//
// Every LLM judgment is stored under sha256(prompt_version + stage + inputs).
// Identical inputs never reach the model twice, so a completed run re-plays
// bit-for-bit, and re-runs after small edits only re-judge what changed.
// (temperature 0 is also requested, but `claude -p` behind the proxy makes no
// hard determinism promise — the cache is what does.)
//
// sha256 shells out to openssl (BAML has no crypto stdlib), same trade-off as
// agent-tries-baml's hmac.baml: openssl is short-lived with no grandchildren,
// so plain sys.exec pipe capture is safe.

function sha256_hex(message: string) -> string {
    let out = baml
        .sys
        .exec(
            "openssl",
            ["dgst", "-sha256", "-r"],
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
    // "abc123... *stdin\n" -> "abc123..."
    let space = text.index_of(" ");
    if (space > 0) {
        text.substring(0, space)
    } else {
        text.trim()
    }
}

function cache_key(stage: string, payload: string) -> string {
    sha256_hex(prompt_version() + "\n--stage--\n" + stage + "\n--payload--\n" + payload)
}

function cache_path(cache_dir: string, key: string) -> string {
    cache_dir + "/" + key + ".json"
}

function cache_get(cache_dir: string, key: string) -> string? {
    if (key == "") {
        return null;
    }
    let p = cache_path(cache_dir, key);
    if (baml.fs.exists(p)) {
        baml.fs.read(p) catch (_) {
            _ => null
        }
    } else {
        null
    }
}

function cache_put(cache_dir: string, key: string, value: string) -> null {
    if (key == "") {
        return null;
    }
    baml.fs.mkdir(cache_dir, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    baml.fs.write(cache_path(cache_dir, key), value) catch (_) {
        _ => null
    };
    null
}
