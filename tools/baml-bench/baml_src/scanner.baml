// Stage (a) — deterministic scanning of a target BAML project into CodeBlocks.
//
// Walks the target for .baml files, scans each for top-level declarations
// (column-0 keyword) with a brace/string/comment-aware state machine, then
// RECURSES into declaration bodies for members: class fields and methods, enum
// variants, function params and let-bound variables. No compiler dependency,
// no LLM: same input bytes -> same blocks, always.
//
// Id scheme (stable across runs, string-map-safe):
//   PROGRAM
//   F:<relpath>
//   D:<relpath>:<Name>                (top-level decl; "#2" suffix on collision)
//   M:<relpath>:<Owner>.<member>      (field / variant / param / method)
//   V:<relpath>:<Owner>.<name>#<n>    (let binding; ordinal disambiguates shadowing)

function pad6(n: int) -> string {
    let s = i2s(n);
    while (s.length() < 6) {
        s = "0" + s;
    }
    s
}

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

// Per-line brace-depth and paren-depth deltas, ignoring braces/parens inside
// "..." strings, #"..."# raw strings, and // comments. `state` carries
// multi-line raw-string mode across lines: "" = code, "raw" = inside #"..."#.
// Paren tracking is what keeps a multi-line function signature (open "(" with
// no "{" yet) from looking like a closed brace-less declaration.
class LineScan {
    delta: int,
    pdelta: int,
    state: string,
}

function scan_line(line: string, state: string) -> LineScan {
    let depth = 0;
    let pdepth = 0;
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
                            if (ch == "(") {
                                pdepth = pdepth + 1;
                            }
                            if (ch == ")") {
                                pdepth = pdepth - 1;
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
    return LineScan { delta: depth, pdelta: pdepth, state: carry };
}

// The trimmed code portion of a line with any trailing // comment removed
// (string-aware), so end-of-statement checks see ";" and not comment text.
function code_trim(line: string, state: string) -> string {
    if (state != "") {
        return "";
    }
    let out = "";
    let mode = "";
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
        if (mode == "str") {
            out = out + ch;
            if (ch == "\\") {
                if (next != "") {
                    out = out + next;
                }
                i = i + 2;
            } else {
                if (ch == "\"") {
                    mode = "";
                }
                i = i + 1;
            }
        } else {
            if (ch == "/" && next == "/") {
                i = n;
            } else {
                if (ch == "\"") {
                    mode = "str";
                }
                out = out + ch;
                i = i + 1;
            }
        }
    }
    out.trim()
}

// Top-level declaration keywords that open a block.
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

// Depth-2 block kind for a declaration. `impl` blocks classify as "class":
// they are member containers exactly like classes for our purposes.
function classify_decl(kw: string, source: string) -> string {
    match (kw) {
        "test" => "test",
        "testset" => "test",
        "class" => "class",
        "impl" => "class",
        "enum" => "enum",
        "interface" => "interface",
        "type" => "type_alias",
        "client" => "config",
        "retry_policy" => "config",
        "template_string" => "config",
        _ => {
            // An LLM function declares `client:` and `prompt:` as body lines.
            // Only look at code-mode lines: a function that merely mentions
            // "client:" in a string literal is not an LLM function.
            let has_client = false;
            let has_prompt = false;
            let state = "";
            for (let line in source.lines()) {
                if (state == "") {
                    let t = code_trim(line, state);
                    if (t.starts_with("client:") || t.starts_with("client ")) {
                        has_client = true;
                    }
                    if (t.starts_with("prompt:") || t.starts_with("prompt #\"")) {
                        has_prompt = true;
                    }
                }
                let scan = scan_line(line, state);
                state = scan.state;
            }
            if (has_client && has_prompt) {
                "llm_function"
            } else {
                "function"
            }
        },
    }
}

// Scan one file's source into depth-2 declaration blocks. `rel` is the file's
// path relative to the root. Ids collide (class Foo + impl Foo) get "#2"…
function scan_file_decls(rel: string, source: string) -> CodeBlock[] {
    let blocks: CodeBlock[] = [];
    let used: map<string, int> = {};
    let lines = source.lines();
    let state = "";
    let depth = 0;
    let pdepth = 0;
    let seen_brace = false;
    let cur_kw = "";
    let cur_name = "";
    let cur_start = 0;
    let cur_src = "";
    let lineno = 0;
    for (let line in lines) {
        lineno = lineno + 1;
        let opened = false;
        if (depth == 0 && pdepth == 0 && state == "" && cur_kw == "") {
            let kw = decl_keyword(line);
            if (kw != "") {
                cur_kw = kw;
                cur_name = decl_name(line, kw);
                cur_start = lineno;
                cur_src = "";
                seen_brace = false;
                opened = true;
            }
        }
        let scan = scan_line(line, state);
        state = scan.state;
        depth = depth + scan.delta;
        pdepth = pdepth + scan.pdelta;
        if (depth > 0) {
            seen_brace = true;
        }
        if (cur_kw != "") {
            cur_src = cur_src + line + "\n";
            // A decl closes when brace depth returns to 0 after its body — or on
            // the opening line itself for brace-less decls (type aliases). An
            // open paren (multi-line signature) keeps the decl open either way.
            let settled = depth == 0 && pdepth == 0 && state == "";
            let braceless = opened && !line.includes("{") && settled;
            let closed = !opened && settled && seen_brace;
            if (braceless || closed || (opened && line.includes("{") && settled)) {
                let base_id = "D:" + rel + ":" + cur_name;
                let seen = match (used.get(base_id)) {
                    let n: int => n,
                    null => 0,
                };
                used.set(base_id, seen + 1);
                let id = if (seen == 0) {
                    base_id
                } else {
                    base_id + "#" + i2s(seen + 1)
                };
                let none: string[] = [];
                blocks
                    .push(
                        CodeBlock {
                            id: id,
                            parent_id: "F:" + rel,
                            kind: classify_decl(cur_kw, cur_src),
                            name: cur_name,
                            file: rel,
                            start_line: cur_start,
                            end_line: lineno,
                            source: cur_src,
                            child_ids: none,
                            depth: 2,
                        },
                    );
                cur_kw = "";
                cur_name = "";
                cur_src = "";
            }
        }
    }
    blocks
}

// ---- member scanning ----------------------------------------------------------

// True when a trimmed rel-depth-1 line inside a class/interface looks like a
// field declaration: leading identifier immediately followed by ":".
function field_name_of(t: string) -> string {
    if (t.starts_with("//") || t.starts_with("@") || t == "" || t == "}") {
        return "";
    }
    let name = "";
    for (let ch in t.split("")) {
        if (ch == "") {
            // padding from split; skip
        } else {
            if (is_ident_char(ch)) {
                name = name + ch;
            } else {
                if (ch == ":" && (name.length() > 0)) {
                    return name;
                }
                return "";
            }
        }
    }
    ""
}

// Enum variant: a bare identifier line (optionally trailing "," / attributes).
function variant_name_of(t: string) -> string {
    if (t.starts_with("//") || t.starts_with("@") || t == "" || t == "}") {
        return "";
    }
    let name = "";
    let i = 0;
    for (let ch in t.split("")) {
        if (ch == "") {
            // padding; skip
        } else {
            if (i == name.length() && is_ident_char(ch)) {
                name = name + ch;
            }
            i = i + 1;
        }
    }
    // whole leading token must be the identifier; reject "foo: int" etc.
    if (name.length() == 0) {
        return "";
    }
    let rest = t.substring(name.length(), t.length()).trim();
    if (rest == "" || rest == "," || rest.starts_with("@") || rest.starts_with("//")) {
        name
    } else {
        ""
    }
}

// Members of a class/interface/impl decl: fields (and methods, recursed for
// their own params/vars). Emits the full flat descendant list.
function class_members(decl: CodeBlock) -> CodeBlock[] {
    let out: CodeBlock[] = [];
    let state = "";
    let depth = 0;
    let pdepth = 0;
    let lineno = decl.start_line - 1;
    // one open member at a time; a member closes when depth returns to 1
    let mem_kind = ""; // "" | "field" | "method"
    let mem_name = "";
    let mem_start = 0;
    let mem_src = "";
    let mem_opened_body = false; // method reached its "{" (depth > 1 observed)
    for (let line in decl.source.lines()) {
        lineno = lineno + 1;
        let at_depth = depth;
        let in_code = state == "";
        let t = line.trim();
        if (in_code && at_depth == 1 && pdepth == 0 && mem_kind == "") {
            if (t.starts_with("function ")) {
                mem_kind = "method";
                mem_name = decl_name(t, "function");
                mem_start = lineno;
                mem_src = "";
                mem_opened_body = false;
            } else {
                let fname = field_name_of(t);
                if (fname != "") {
                    mem_kind = "field";
                    mem_name = fname;
                    mem_start = lineno;
                    mem_src = "";
                    mem_opened_body = false;
                }
            }
        }
        let scan = scan_line(line, state);
        state = scan.state;
        depth = depth + scan.delta;
        pdepth = pdepth + scan.pdelta;
        if (depth > 1) {
            mem_opened_body = true;
        }
        if (mem_kind != "") {
            mem_src = mem_src + line + "\n";
            // fields settle when depth/parens return to the class level; methods
            // additionally need their body brace (or a ";"-terminated signature,
            // for interfaces) so a multi-line signature stays open.
            let code = code_trim(line, state);
            let method_done = mem_opened_body || code.ends_with(";") || code.ends_with("}");
            if ((depth <= 1) && (pdepth == 0) && (state == "") && (mem_kind == "field" || method_done)) {
                let owner = decl.name;
                let none: string[] = [];
                let member = CodeBlock {
                    id: "M:" + decl.file + ":" + owner + "." + mem_name,
                    parent_id: decl.id,
                    kind: mem_kind,
                    name: mem_name,
                    file: decl.file,
                    start_line: mem_start,
                    end_line: lineno,
                    source: mem_src,
                    child_ids: none,
                    depth: decl.depth + 1,
                };
                out.push(member);
                if (mem_kind == "method") {
                    for (let sub in fn_members(member, owner + "." + mem_name)) {
                        out.push(sub);
                    }
                }
                mem_kind = "";
                mem_name = "";
                mem_src = "";
            }
        }
    }
    out
}

// Variants of an enum decl.
function enum_variants(decl: CodeBlock) -> CodeBlock[] {
    let out: CodeBlock[] = [];
    let state = "";
    let depth = 0;
    let lineno = decl.start_line - 1;
    for (let line in decl.source.lines()) {
        lineno = lineno + 1;
        let at_depth = depth;
        let in_code = state == "";
        if (in_code && at_depth == 1) {
            let vname = variant_name_of(line.trim());
            if (vname != "") {
                let none: string[] = [];
                out.push(
                    CodeBlock {
                        id: "M:" + decl.file + ":" + decl.name + "." + vname,
                        parent_id: decl.id,
                        kind: "variant",
                        name: vname,
                        file: decl.file,
                        start_line: lineno,
                        end_line: lineno,
                        source: line + "\n",
                        child_ids: none,
                        depth: decl.depth + 1,
                    },
                );
            }
        }
        let scan = scan_line(line, state);
        state = scan.state;
        depth = depth + scan.delta;
    }
    out
}

// The signature's parenthesized parameter text plus its (0-based, relative)
// line span within the decl source.
class SigScan {
    text: string,
    start_rel: int,
    end_rel: int,
}

// Extract the first top-level "(...)" from the decl source — the parameter
// list. String/comment aware; paren-depth tracked so nested parens in default
// values survive.
function param_signature(source: string) -> SigScan {
    let text = "";
    let mode = ""; // "" | "str" | "raw"
    let pdepth = 0;
    let started = false;
    let rel = 0 - 1;
    let start_rel = 0;
    for (let line in source.lines()) {
        rel = rel + 1;
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
                    if (started) {
                        text = text + ch;
                    }
                    i = i + 1;
                }
            } else {
                if (mode == "str") {
                    if (started) {
                        text = text + ch;
                    }
                    if (ch == "\\") {
                        if (started && next != "") {
                            text = text + next;
                        }
                        i = i + 2;
                    } else {
                        if (ch == "\"") {
                            mode = "";
                        }
                        i = i + 1;
                    }
                } else {
                    if (ch == "/" && next == "/") {
                        i = n;
                    } else {
                        if (ch == "#" && next == "\"") {
                            mode = "raw";
                            i = i + 2;
                        } else {
                            if (ch == "\"") {
                                mode = "str";
                                if (started) {
                                    text = text + ch;
                                }
                                i = i + 1;
                            } else {
                                if (ch == "(") {
                                    if (!started) {
                                        started = true;
                                        start_rel = rel;
                                    } else {
                                        text = text + ch;
                                    }
                                    pdepth = pdepth + 1;
                                    i = i + 1;
                                } else {
                                    if (ch == ")") {
                                        pdepth = pdepth - 1;
                                        if (started && pdepth == 0) {
                                            return SigScan { text: text, start_rel: start_rel, end_rel: rel };
                                        }
                                        if (started) {
                                            text = text + ch;
                                        }
                                        i = i + 1;
                                    } else {
                                        if (started) {
                                            text = text + ch;
                                        }
                                        i = i + 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if (started) {
            text = text + "\n";
        }
    }
    return SigScan { text: "", start_rel: 0, end_rel: 0 };
}

// Split parameter text on top-level commas (paren/angle/bracket/brace aware,
// string-aware for defaults).
function split_params(sig: string) -> string[] {
    let out: string[] = [];
    let cur = "";
    let depth = 0;
    let in_str = false;
    let prev = "";
    for (let ch in sig.split("")) {
        if (ch == "") {
            // padding; skip
        } else {
            if (in_str) {
                cur = cur + ch;
                if (ch == "\"" && prev != "\\") {
                    in_str = false;
                }
            } else {
                if (ch == "\"") {
                    in_str = true;
                    cur = cur + ch;
                } else {
                    if (ch == "(" || ch == "<" || ch == "[" || ch == "{") {
                        depth = depth + 1;
                        cur = cur + ch;
                    } else {
                        if (ch == ")" || ch == ">" || ch == "]" || ch == "}") {
                            depth = depth - 1;
                            cur = cur + ch;
                        } else {
                            if (ch == "," && depth == 0) {
                                out.push(cur);
                                cur = "";
                            } else {
                                cur = cur + ch;
                            }
                        }
                    }
                }
            }
            prev = ch;
        }
    }
    if (cur.trim() != "") {
        out.push(cur);
    }
    out
}

// Leading identifier of a trimmed string.
function leading_ident(t: string) -> string {
    let name = "";
    for (let ch in t.split("")) {
        if (ch == "") {
            // padding; skip
        } else {
            if (is_ident_char(ch)) {
                name = name + ch;
            } else {
                return name;
            }
        }
    }
    name
}

// True when a trimmed code line declares a variable: starts with "let " and
// carries a top-level " = " assignment BEFORE any "=>" (which would make it a
// match-arm binding pattern instead).
function is_var_line(t: string) -> bool {
    if (!t.starts_with("let ")) {
        return false;
    }
    let eq = t.index_of(" = ");
    if (eq < 0) {
        return false;
    }
    let arrow = t.index_of("=>");
    (arrow < 0) || (eq < arrow)
}

// Params and let-bound vars of a function-like decl (function, llm_function,
// test, method). `owner` is the dotted owner path for ids. One open var at a
// time: lets nested inside another let's initializer are folded into the outer
// var's span (rare; the outer intention covers them).
function fn_members(decl: CodeBlock, owner: string) -> CodeBlock[] {
    let out: CodeBlock[] = [];
    let child_depth = decl.depth + 1;

    // ---- params from the signature ----
    let sig = param_signature(decl.source);
    if (sig.text.trim() != "") {
        for (let piece in split_params(sig.text)) {
            let t = piece.trim();
            let pname = leading_ident(t);
            if (pname != "" && pname != "self") {
                let none: string[] = [];
                out.push(
                    CodeBlock {
                        id: "M:" + decl.file + ":" + owner + "." + pname,
                        parent_id: decl.id,
                        kind: "param",
                        name: pname,
                        file: decl.file,
                        start_line: decl.start_line + sig.start_rel,
                        end_line: decl.start_line + sig.end_rel,
                        source: t + "\n",
                        child_ids: none,
                        depth: child_depth,
                    },
                );
            }
        }
    }

    // ---- let-bound vars ----
    let counts: map<string, int> = {};
    let state = "";
    let depth = 0;
    let pdepth = 0;
    let lineno = decl.start_line - 1;
    let var_name = "";
    let var_start = 0;
    let var_src = "";
    let var_depth = 0;
    let var_pdepth = 0;
    for (let line in decl.source.lines()) {
        lineno = lineno + 1;
        let at_depth = depth;
        let at_pdepth = pdepth;
        let in_code = state == "";
        let t = line.trim();
        if (in_code && var_name == "" && (at_depth >= 1) && is_var_line(t)) {
            let vname = leading_ident(t.substring(4, t.length()).trim());
            if (vname != "") {
                var_name = vname;
                var_start = lineno;
                var_src = "";
                var_depth = at_depth;
                var_pdepth = at_pdepth;
            }
        }
        let scan = scan_line(line, state);
        state = scan.state;
        depth = depth + scan.delta;
        pdepth = pdepth + scan.pdelta;
        if (var_name != "") {
            var_src = var_src + line + "\n";
            let code = code_trim(line, state);
            if ((depth <= var_depth) && (pdepth <= var_pdepth) && (state == "") && code.ends_with(";")) {
                let seen = match (counts.get(var_name)) {
                    let n: int => n,
                    null => 0,
                };
                counts.set(var_name, seen + 1);
                let none: string[] = [];
                out.push(
                    CodeBlock {
                        id: "V:" + decl.file + ":" + owner + "." + var_name + "#" + i2s(seen + 1),
                        parent_id: decl.id,
                        kind: "var",
                        name: var_name,
                        file: decl.file,
                        start_line: var_start,
                        end_line: lineno,
                        source: var_src,
                        child_ids: none,
                        depth: child_depth,
                    },
                );
                var_name = "";
                var_src = "";
            }
        }
    }
    out
}

// All descendant blocks of a depth-2 declaration (flat list, parent_id set).
function members_of(decl: CodeBlock) -> CodeBlock[] {
    match (decl.kind) {
        "class" => class_members(decl),
        "interface" => class_members(decl),
        "enum" => enum_variants(decl),
        "function" => fn_members(decl, decl.name),
        "llm_function" => fn_members(decl, decl.name),
        "test" => fn_members(decl, decl.name),
        _ => {
            // type_alias / config: no children
            let none: CodeBlock[] = [];
            none
        },
    }
}
