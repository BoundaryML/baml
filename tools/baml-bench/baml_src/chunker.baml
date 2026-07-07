// Stage (a) — deterministic chunking of a target BAML project.
//
// Walks the target for .baml files and scans each one for top-level declarations
// (column-0 keyword), tracking brace depth with a small string/comment-aware
// state machine so a chunk spans its full block. No compiler dependency, no
// LLM: same input bytes -> same chunks, always.

// Recursive .baml file listing, sorted, as paths relative to `root`.
function list_baml_files(root: string) -> string[] {
    let out: string[] = [];
    collect_baml_files(root, "", out);
    out.sort_by_key(
        (p: string) -> string {
            p
        },
    )
}

function collect_baml_files(root: string, rel: string, out: string[]) -> null {
    let dir = if (rel == "") {
        root
    } else {
        root + "/" + rel
    };
    let entries = baml.fs.read_dir(dir) catch (_) {
        _ => {
            return null;
        }
    };
    for (let e in entries) {
        if (e.name.starts_with(".") || e.name == "node_modules") {
            // skip hidden dirs (.baml toolchain dir, .git, caches) and vendored deps
        } else {
            let child = if (rel == "") {
                e.name
            } else {
                rel + "/" + e.name
            };
            if (e.is_dir) {
                collect_baml_files(root, child, out);
            } else {
                if (e.name.ends_with(".baml")) {
                    out.push(child);
                }
            }
        }
    }
    null
}

// Per-line brace-depth delta, ignoring braces inside "..." strings, #"..."#
// raw strings, and // comments. `state` carries multi-line raw-string mode
// across lines: "" = code, "raw" = inside #"..."#.
class LineScan {
    delta: int,
    state: string,
}

function scan_line(line: string, state: string) -> LineScan {
    let depth = 0;
    let mode = state; // "" | "str" | "raw"
    let chars = line.split("");
    let i = 0;
    let n = chars.length();
    while (i < n) {
        let ch = chars.at(i) catch (_) {
            _ => ""
        };
        let next = if (i + 1 < n) {
            chars.at(i + 1) catch (_) {
                _ => ""
            }
        } else {
            ""
        };
        if (mode == "raw") {
            if (ch == "\"" && next == "#") {
                mode = "";
                i = i + 2;
            } else {
                i = i + 1;
            }
        } else {
            if (mode == "str") {
                if (ch == "\\") {
                    i = i + 2;
                } else {
                    if (ch == "\"") {
                        mode = "";
                    }
                    i = i + 1;
                }
            } else {
                // code
                if (ch == "/" && next == "/") {
                    i = n; // rest of line is comment
                } else {
                    if (ch == "#" && next == "\"") {
                        mode = "raw";
                        i = i + 2;
                    } else {
                        if (ch == "\"") {
                            mode = "str";
                            i = i + 1;
                        } else {
                            if (ch == "{") {
                                depth = depth + 1;
                            }
                            if (ch == "}") {
                                depth = depth - 1;
                            }
                            i = i + 1;
                        }
                    }
                }
            }
        }
    }
    // an unterminated plain string never spans lines in BAML; reset to code
    let carry = if (mode == "str") {
        ""
    } else {
        mode
    };
    return LineScan { delta: depth, state: carry };
}

// Top-level declaration keywords that open a chunk.
function decl_keyword(line: string) -> string {
    let kws = [
        "function",
        "class",
        "enum",
        "interface",
        "type",
        "testset",
        "test",
        "client",
        "retry_policy",
        "template_string",
        "impl",
    ];
    for (let kw in kws) {
        if (line.starts_with(kw + " ") || line.starts_with(kw + "<")) {
            return kw;
        }
    }
    ""
}

// Declaration name: first identifier (or quoted title, for tests) after the
// keyword, skipping generics on the keyword itself (client<llm>).
function decl_name(line: string, kw: string) -> string {
    let rest = line.substring(kw.length(), line.length()).trim();
    if (rest.starts_with("<")) {
        let close = rest.index_of(">");
        if (close >= 0) {
            rest = rest.substring(close + 1, rest.length()).trim();
        }
    }
    if (rest.starts_with("\"")) {
        let tail = rest.substring(1, rest.length());
        let endq = tail.index_of("\"");
        if (endq >= 0) {
            return tail.substring(0, endq);
        }
    }
    let name = "";
    for (let ch in rest.split("")) {
        if (ch == "") {
            // split("") pads with empty edge elements; skip them
        } else {
            if (is_ident_char(ch) || ch == "$") {
                name = name + ch;
            } else {
                return name;
            }
        }
    }
    name
}

function classify_chunk(kw: string, source: string) -> string {
    match (kw) {
        "test" => "test",
        "testset" => "test",
        "class" => "type",
        "enum" => "type",
        "interface" => "type",
        "type" => "type",
        "client" => "config",
        "retry_policy" => "config",
        "template_string" => "config",
        _ => {
            let has_client = source.includes("client:") || source.includes("client ");
            let has_prompt = source.includes("prompt:")
                || source.includes("prompt #\"")
                || source.includes("prompt: #\"");
            if (has_client && has_prompt) {
                "llm_function"
            } else {
                "function"
            }
        },
    }
}

// Chunk one file's source. `rel` is the file's path relative to the root.
function chunk_file(rel: string, source: string) -> Chunk[] {
    let chunks: Chunk[] = [];
    let lines = source.lines();
    let state = "";
    let depth = 0;
    let cur_kw = "";
    let cur_name = "";
    let cur_start = 0;
    let cur_src = "";
    let lineno = 0;
    for (let line in lines) {
        lineno = lineno + 1;
        let opened = false;
        if (depth == 0 && state == "" && cur_kw == "") {
            let kw = decl_keyword(line);
            if (kw != "") {
                cur_kw = kw;
                cur_name = decl_name(line, kw);
                cur_start = lineno;
                cur_src = "";
                opened = true;
            }
        }
        let scan = scan_line(line, state);
        state = scan.state;
        depth = depth + scan.delta;
        if (cur_kw != "") {
            cur_src = cur_src + line + "\n";
            // A decl closes when depth returns to 0 after its line — or on the
            // opening line itself for brace-less decls (type aliases).
            let braceless = opened && !line.includes("{") && depth == 0 && state == "";
            let closed = !opened && depth == 0 && state == "";
            if (braceless || closed || (opened && line.includes("{") && depth == 0 && state == "")) {
                chunks
                    .push(
                        Chunk {
                            id: rel + ":" + cur_name,
                            file: rel,
                            name: cur_name,
                            kind: classify_chunk(cur_kw, cur_src),
                            start_line: cur_start,
                            end_line: lineno,
                            source: cur_src,
                        },
                    );
                cur_kw = "";
                cur_name = "";
                cur_src = "";
            }
        }
    }
    chunks
}

// Chunk the whole target project, sorted by (file, start_line).
function chunk_project(root: string) -> Chunk[] {
    let all: Chunk[] = [];
    for (let rel in list_baml_files(root)) {
        let src = baml.fs.read(root + "/" + rel) catch (_) {
            _ => ""
        };
        if (src != "") {
            for (let c in chunk_file(rel, src)) {
                all.push(c);
            }
        }
    }
    all.sort_by_key(
        (c: Chunk) -> string {
            c.file + "#" + pad6(c.start_line)
        },
    )
}

function pad6(n: int) -> string {
    let s = i2s(n);
    while (s.length() < 6) {
        s = "0" + s;
    }
    s
}
