// The program store of this site (contract section 9.5).
//
// The store is content-addressed and immutable. A program is identified by the
// SHA-256 of its Borsh bytes, the value that a snapshot header and the `hello`
// event carry. Workers write the entries. The site server reads an entry to
// serve it to another site, and writes an entry that it fetched from another
// site before it starts a worker for a program that it does not hold.
//
// File layout, `<store>/<first two hex chars>/<hash>.bamlprog`:
//
//   magic           8 bytes   "BAMLPROG"
//   format_version  u32 LE
//   build_len       u32 LE
//   runtime_build   build_len bytes, UTF-8
//   payload_len     u64 LE
//   program         payload_len bytes, the rest of the file: the Borsh bytes
//                   that the hash covers
//
// This is the entry layout of the Rust crate `bex_program_store`, which the
// worker uses, in format version 1. The server reads and writes nothing else:
// an entry in another layout could not be used by a worker of this tree, so it
// counts as missing. The crate also keeps build marker files next to an entry
// (`<hash>.<build tag>.build`). The server does not write markers, because a
// worker accepts an entry whose header names its own runtime build.
//
// A fetched entry gets a header that this server builds itself from the
// `runtime_build` that the sending site reported. Header bytes from another
// site are not trusted: only the program bytes are covered by the hash.
//
// Known limit: the server writes an entry without fsync (the worker's writer
// syncs the file and the directory). A torn entry fails its hash check and is
// fetched again.

/// A parsed store entry.
class ProgramFile {
    format_version: int,
    runtime_build: string,
    /// Every byte before the program bytes.
    header: uint8array,
    /// The bytes that the program hash covers.
    program: uint8array,
}

/// True when `hash` is 64 lowercase hexadecimal characters. The value becomes
/// part of a file name, so nothing else is accepted.
function valid_program_hash(hash: string) -> bool throws never {
    if (hash.length() != 64) {
        return false;
    }
    let allowed = "0123456789abcdef";
    for (let ch in hash.chars()) {
        if (!allowed.includes(ch)) {
            return false;
        }
    }
    true
}

function program_dir_of(app: App, hash: string) -> string throws never {
    `${app.cfg.programs_dir}/${hash.slice(0, 2)}`
}

function program_path(app: App, hash: string) -> string throws never {
    `${program_dir_of(app, hash)}/${hash}.bamlprog`
}

function sha256_hex(bytes: uint8array) -> string throws never {
    let h = baml.crypto.Sha256.new();
    h.update(bytes);
    h.finish().to_hex()
}

/// The little-endian u32 at `offset`, or -1 when the input is too short.
function u32_le(bytes: uint8array, offset: int) -> int throws never {
    if (offset < 0 || offset + 4 > bytes.length()) {
        return -1;
    }
    let value = 0;
    let i = 3;
    while (i >= 0) {
        value = value * 256 + (bytes.at(offset + i) ?? 0);
        i -= 1;
    }
    value
}

/// The little-endian u64 at `offset`, or -1 when the input is too short or
/// the value does not fit a non-negative int.
function u64_le(bytes: uint8array, offset: int) -> int throws never {
    let low = u32_le(bytes, offset);
    let high = u32_le(bytes, offset + 4);
    if (low < 0 || high < 0 || high >= 2097152) {
        return -1;
    }
    high * 4294967296 + low
}

function u32_le_bytes(value: int) -> uint8array throws never {
    le_bytes(value, 4)
}

function le_bytes(value: int, width: int) -> uint8array throws never {
    let out: int[] = [];
    let rest = value;
    let i = 0;
    while (i < width) {
        out.push(rest % 256);
        rest = rest / 256;
        i += 1;
    }
    baml.Uint8Array.from_array(out) catch_all (e) {
        _ => b""
    }
}

/// The header of a store entry in the layout at the top of this file.
function program_header(
    format_version: int,
    runtime_build: string,
    payload_len: int,
) -> uint8array throws never {
    let build = runtime_build.to_utf8();
    "BAMLPROG".to_utf8()
        .concat(u32_le_bytes(format_version))
        .concat(u32_le_bytes(build.length()))
        .concat(build)
        .concat(le_bytes(payload_len, 8))
}

/// True when `build` can be the runtime build of a store entry: not empty, at
/// most 256 bytes, and one line. The same rule as `check_build` of the crate
/// `bex_program_store`.
function valid_runtime_build(build: string) -> bool throws never {
    let n = build.to_utf8().length();
    n > 0 && n <= 256 && !build.includes("\n") && !build.includes("\r")
}

/// Splits a store entry into header and program bytes and verifies the hash of
/// the program bytes. Returns null when the bytes are not a format version 1
/// entry for `hash`. The hash is computed once, over the bytes that the header
/// announces.
function parse_program_file(bytes: uint8array, hash: string) -> ProgramFile? throws never {
    let n = bytes.length();
    if (n < 24 || bytes.slice(0, 8).to_string() != "BAMLPROG") {
        return null;
    }
    let version = u32_le(bytes, 8);
    let build_len = u32_le(bytes, 12);
    if (version != 1 || build_len < 1 || build_len > 256) {
        return null;
    }
    let start = 24 + build_len;
    if (start > n || u64_le(bytes, 16 + build_len) != n - start) {
        return null;
    }
    let build = bytes.slice(16, 16 + build_len).to_string();
    if (!valid_runtime_build(build)) {
        return null;
    }
    let program = bytes.slice(start, n);
    if (sha256_hex(program) != hash) {
        return null;
    }
    ProgramFile {
        format_version: version,
        runtime_build: build,
        header: bytes.slice(0, start),
        program: program,
    }
}

/// The path of the marker with which a worker of `build` records that it
/// stored the entry `hash` (see `bex_program_store`).
function marker_path(app: App, hash: string, build: string) -> string throws never {
    `${program_dir_of(app, hash)}/${hash}.${sha256_hex(build.to_utf8()).slice(0, 16)}.build`
}

/// Reads and verifies the entry for `hash` in the store of this site. A file
/// that fails the check counts as missing.
function read_program(app: App, hash: string) -> ProgramFile? throws never {
    let bytes = read_bytes(program_path(app, hash)) catch_all (e) {
        _ => null
    };
    if (bytes == null) {
        return null;
    }
    let parsed = parse_program_file(bytes, hash);
    if (parsed == null) {
        warn(
            `the program store entry ${program_path(app, hash)} does not match its hash; treating it as missing`,
        );
    }
    parsed
}

/// `GET /api/programs/:hash`.
function get_program(app: App, hash: string) -> baml.http.Response throws never {
    if (!valid_program_hash(hash)) {
        return fail(400, "a program hash is 64 lowercase hexadecimal characters");
    }
    let entry = read_program(app, hash);
    if (entry == null) {
        return fail(404, `site ${app.cfg.site} does not hold the program ${hash}`);
    }
    // An entry is usable by every build that stored the bytes. The header
    // names only the first of them, and each later one left a marker. The
    // workers of this site are the ones that can vouch for the entry, so their
    // build is reported when their marker exists.
    let build = entry.runtime_build;
    let own = app.worker_build;
    if (own.length() > 0 && own != build) {
        let marked = baml.fs.exists(marker_path(app, hash, own)) catch_all (e) {
            _ => false
        };
        if (marked) {
            build = own;
        }
    }
    respond(
        200,
        {
            "hash": hash,
            "runtime_build": build,
            "program_base64": entry.program.to_base64(),
            "format_version": entry.format_version,
        },
    )
}

/// Makes sure that the store of this site holds the program `hash` before a
/// worker starts for a run that came from `from_site`. Returns null when the
/// program is there, and a message otherwise. A null or empty hash means that
/// the sender runs a worker without a program store; nothing is fetched then.
function ensure_program(app: App, hash: string?, from_site: string) -> string? throws never {
    if (hash == null || app.cfg.legacy_worker) {
        return null;
    }
    let h: string = hash;
    if (h.length() == 0) {
        return null;
    }
    if (!valid_program_hash(h)) {
        return `the program hash ${h} is not 64 lowercase hexadecimal characters`;
    }
    // Another request may be fetching the same program right now. One request
    // claims the fetch under the state lock, and the others wait for it.
    let waited = 0;
    while (waited < 40000) {
        // The cache saves the hash check of an entry that was verified
        // before. It is only trusted while the file is still there.
        if (app.programs.has(h)) {
            let there = baml.fs.exists(program_path(app, h)) catch_all (e) {
                _ => false
            };
            if (!there) {
                locked(app, () -> {
                    app.programs.delete(h);
                    null
                });
            }
        }
        let state = locked(app, () -> {
            if (app.programs.has(h)) {
                "have"
            } else if (app.fetching.has(h)) {
                "wait"
            } else {
                app.fetching.set(h, true);
                "fetch"
            }
        });
        if (state == "have") {
            return null;
        }
        if (state == "fetch") {
            let failure = fetch_program(app, h, from_site);
            locked(app, () -> {
                app.fetching.delete(h);
                if (failure == null) {
                    app.programs.set(h, true);
                }
                null
            });
            return failure;
        }
        pause_ms(25);
        waited += 25;
    }
    `timed out waiting for another request to fetch the program ${h}`
}

function fetch_program(app: App, hash: string, from_site: string) -> string? throws never {
    if (read_program(app, hash) != null) {
        return null;
    }
    if (from_site == app.cfg.site || from_site.length() == 0) {
        return `site ${app.cfg.site} does not hold the program ${hash}, and the request names no other site to fetch it from`;
    }
    let base = site_url(app.cfg, from_site);
    if (base == null) {
        return `cannot fetch the program ${hash}: site ${from_site} has no entry in SITES`;
    }
    //# First use of this program here: fetch it from the site that sent the run
    pause_ms(app.cfg.chaos.program_fetch_delay_ms);
    let started = now_ms();
    let res = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: `${base}/api/programs/${hash}`,
            headers: {},
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(30n),
    ) catch_all (e, ctx) {
        _ => null
    };
    if (res == null) {
        return `could not reach site ${from_site} to fetch the program ${hash}`;
    }
    let text = res.text() catch_all (e) {
        _ => ""
    };
    if (!res.ok()) {
        return `site ${from_site} did not serve the program ${hash} (${res.status_code}): ${text}`;
    }
    let body = jparse(text);
    let program = baml.Uint8Array.from_base64(jget_str(body, "program_base64")) catch_all (e) {
        _ => b""
    };
    //# Verify the hash of the received bytes before anything is stored
    let actual = sha256_hex(program);
    if (program.length() == 0 || actual != hash) {
        return `the program that site ${from_site} served for ${hash} has the hash ${actual}; it was not stored`;
    }
    //# The header is built here; header bytes of another site are not trusted
    let build = jget_str(body, "runtime_build");
    if (!valid_runtime_build(build)) {
        return `site ${from_site} served the program ${hash} without a usable runtime build; it was not stored`;
    }
    // A worker only runs a program that its own build wrote. Refusing the
    // program here tells the caller why, instead of a child that fails later.
    let own = app.worker_build;
    if (own.length() > 0 && own != build) {
        return `site ${from_site} served the program ${hash} of runtime build ${build}, and the workers of site ${app.cfg.site} are build ${own}; it was not stored`;
    }
    let header = program_header(1, build, program.length());
    let failure = store_program(app, hash, header.concat(program));
    if (failure != null) {
        return failure;
    }
    emit_event(
        app,
        {
            "type": "program_fetched",
            "hash": hash,
            "from_site": from_site,
            "bytes": program.length(),
            "ms": now_ms() - started,
        },
    );
    null
}

/// Writes a store entry: a temporary file in the same directory, then a rename.
/// The temporary file has the name pattern of the Rust writer
/// (`.tmp-<hash>.<random>.partial`), so that the crate's cleanup of stale
/// temporary files covers it, and it is removed on every failure path.
function store_program(app: App, hash: string, bytes: uint8array) -> string? throws never {
    let dir = program_dir_of(app, hash);
    ensure_dir(dir);
    let path = program_path(app, hash);
    let suffix = baml.random.SystemRandom.get().random(6).to_hex();
    let tmp = `${dir}/.tmp-${hash}.${suffix}.partial`;
    let wrote = write_program_bytes(tmp, bytes) catch_all (e, ctx) {
        _ => ctx.to_string()
    };
    if (wrote.length() > 0) {
        remove_quietly(tmp);
        return `cannot write ${tmp}: ${wrote}`;
    }
    let moved = baml.sys.exec("mv", ["-f", tmp, path], null) catch_all (e) {
        _ => null
    };
    if (moved == null) {
        remove_quietly(tmp);
        return `cannot move ${tmp} to ${path}`;
    }
    if (moved.exit_code != 0) {
        remove_quietly(tmp);
        return `cannot move ${tmp} to ${path}: ${moved.stderr.to_string()}`;
    }
    null
}

function remove_quietly(path: string) -> void throws never {
    baml.fs.remove(path) catch_all (e) {
        _ => null
    };
}

function write_program_bytes(path: string, bytes: uint8array) -> string throws baml.errors.Io {
    baml.fs.write_bytes(path, bytes);
    ""
}
