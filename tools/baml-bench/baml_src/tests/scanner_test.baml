// Token-free tests for the recursive block scanner and tree assembly — the
// highest-risk deterministic code in the pipeline.

// unwrap helpers with loud sentinels
function nth_block(blocks: CodeBlock[], i: int) -> CodeBlock {
    match (blocks.at(i)) {
        let b: CodeBlock => b,
        null => {
            let none: string[] = [];
            CodeBlock {
                id: "MISSING",
                parent_id: null,
                kind: "",
                name: "MISSING",
                file: "",
                start_line: -1,
                end_line: -1,
                source: "",
                child_ids: none,
                depth: -1,
            }
        },
    }
}

function find_block(blocks: CodeBlock[], id: string) -> CodeBlock {
    for (let b in blocks) {
        if (b.id == id) {
            return b;
        }
    }
    let none: string[] = [];
    CodeBlock {
        id: "MISSING:" + id,
        parent_id: null,
        kind: "",
        name: "MISSING",
        file: "",
        start_line: -1,
        end_line: -1,
        source: "",
        child_ids: none,
        depth: -1,
    }
}

// decls + all members for one source text
function scan_all(rel: string, src: string) -> CodeBlock[] {
    let out: CodeBlock[] = [];
    for (let d in scan_file_decls(rel, src)) {
        out.push(d);
        for (let m in members_of(d)) {
            out.push(m);
        }
    }
    out
}

testset "scanner_decls" {
    test "top-level decls get correct kinds and spans" {
        let src = "// header comment\n"
            + "class User {\n    name: string,\n}\n"
            + "\n"
            + "function greet(u: User) -> string {\n    \"hi \" + u.name\n}\n"
            + "\n"
            + "function Ask(q: string) -> string {\n    client: Grader\n    prompt: #\"\n        {{ q }} has a } brace and a { too\n    \"#\n}\n"
            + "\n"
            + "test \"greet works\" {\n    assert.equal(greet(User { name: \"a\" }), \"hi a\");\n}\n";
        let decls = scan_file_decls("x.baml", src);
        assert.equal(decls.length(), 4);
        let c0 = nth_block(decls, 0);
        assert.equal(c0.id, "D:x.baml:User");
        assert.equal(c0.kind, "class");
        assert.equal(c0.start_line, 2);
        assert.equal(c0.end_line, 4);
        assert.equal(nth_block(decls, 1).kind, "function");
        assert.equal(nth_block(decls, 2).kind, "llm_function");
        let t = nth_block(decls, 3);
        assert.equal(t.kind, "test");
        assert.equal(t.name, "greet works");
    }

    test "multi-line signatures stay open until the body closes" {
        let src = "function f(\n"
            + "    a: string,\n"
            + "    b: int,\n"
            + ") -> string {\n"
            + "    a\n"
            + "}\n";
        let decls = scan_file_decls("m.baml", src);
        assert.equal(decls.length(), 1);
        assert.equal(nth_block(decls, 0).start_line, 1);
        assert.equal(nth_block(decls, 0).end_line, 6);
    }

    test "string-literal mentions of client:/prompt: are not llm_functions" {
        let src = "function looks_llm(source: string) -> bool {\n"
            + "    source.includes(\"client:\") && source.includes(\"prompt:\")\n"
            + "}\n";
        let decls = scan_file_decls("c.baml", src);
        assert.equal(nth_block(decls, 0).kind, "function");
    }

    test "braceless type alias is a single-line type_alias block" {
        let decls = scan_file_decls("z.baml", "type Reply = \"yes\" | \"no\"\n");
        assert.equal(decls.length(), 1);
        assert.equal(nth_block(decls, 0).kind, "type_alias");
        assert.equal(nth_block(decls, 0).name, "Reply");
    }

    test "colliding decl names get ordinal ids" {
        let src = "class Foo {\n    a: int,\n}\n"
            + "impl Foo {\n    function m(self) -> int {\n        1\n    }\n}\n";
        let decls = scan_file_decls("i.baml", src);
        assert.equal(decls.length(), 2);
        assert.equal(nth_block(decls, 0).id, "D:i.baml:Foo");
        assert.equal(nth_block(decls, 1).id, "D:i.baml:Foo#2");
    }
}

testset "scanner_members" {
    test "class fields and methods, with method params and vars" {
        let src = "class WorkQueue {\n"
            + "    next: int,\n"
            + "\n"
            + "    // cooperative-atomic claim\n"
            + "    function take(self, limit: int) -> int {\n"
            + "        let i = self.next;\n"
            + "        i\n"
            + "    }\n"
            + "}\n";
        let all = scan_all("q.baml", src);
        let f = find_block(all, "M:q.baml:WorkQueue.next");
        assert.equal(f.kind, "field");
        assert.equal(f.start_line, 2);
        assert.equal(f.end_line, 2);
        let m = find_block(all, "M:q.baml:WorkQueue.take");
        assert.equal(m.kind, "method");
        assert.equal(m.start_line, 5);
        assert.equal(m.end_line, 8);
        assert.equal(m.depth, 3);
        let p = find_block(all, "M:q.baml:WorkQueue.take.limit");
        assert.equal(p.kind, "param");
        assert.equal(p.depth, 4);
        let v = find_block(all, "V:q.baml:WorkQueue.take.i#1");
        assert.equal(v.kind, "var");
        assert.equal(v.start_line, 6);
    }

    test "multi-line struct default keeps a field open" {
        let src = "class C {\n"
            + "    opts: Options = Options {\n"
            + "        deep: 1,\n"
            + "    },\n"
            + "    tail: int,\n"
            + "}\n";
        let all = scan_all("d.baml", src);
        let f = find_block(all, "M:d.baml:C.opts");
        assert.equal(f.start_line, 2);
        assert.equal(f.end_line, 4);
        assert.equal(find_block(all, "M:d.baml:C.tail").start_line, 5);
    }

    test "enum variants" {
        let src = "enum Color {\n    Red,\n    Green @description(\"g\")\n    // note\n    Blue,\n}\n";
        let all = scan_all("e.baml", src);
        assert.equal(find_block(all, "M:e.baml:Color.Red").kind, "variant");
        assert.equal(find_block(all, "M:e.baml:Color.Blue").start_line, 5);
        // attribute-suffixed variant still parses by leading identifier
        assert.equal(find_block(all, "M:e.baml:Color.Green").kind, "variant");
    }

    test "params from multi-line signatures, self skipped" {
        let src = "function go(\n"
            + "    cache_dir: string,\n"
            + "    items: map<string, int[]>,\n"
            + "    limit: int = 6,\n"
            + ") -> int {\n"
            + "    limit\n"
            + "}\n";
        let all = scan_all("p.baml", src);
        let a = find_block(all, "M:p.baml:go.cache_dir");
        assert.equal(a.kind, "param");
        assert.equal(a.start_line, 1);
        assert.equal(a.end_line, 5);
        // generic commas don't split params
        assert.equal(find_block(all, "M:p.baml:go.items").kind, "param");
        assert.equal(find_block(all, "M:p.baml:go.limit").kind, "param");
    }

    test "vars: shadowing ordinals, trailing comments, block initializers" {
        let src = "function f() -> int {\n"
            + "    let x = 1; // trailing comment\n"
            + "    let y = if (x > 0) {\n"
            + "        2\n"
            + "    } else {\n"
            + "        3\n"
            + "    };\n"
            + "    while (x < 3) {\n"
            + "        let x = 9;\n"
            + "    }\n"
            + "    x\n"
            + "}\n";
        let all = scan_all("v.baml", src);
        let x1 = find_block(all, "V:v.baml:f.x#1");
        assert.equal(x1.start_line, 2);
        assert.equal(x1.end_line, 2);
        let y = find_block(all, "V:v.baml:f.y#1");
        assert.equal(y.start_line, 3);
        assert.equal(y.end_line, 7);
        // the loop-shadowed x is its own var with the next ordinal
        assert.equal(find_block(all, "V:v.baml:f.x#2").start_line, 9);
    }

    test "test blocks get no params from body calls" {
        let src = "test \"age\" {\n    assert.equal(age_days(Order { id: \"x\" }, 86400), 1);\n}\n";
        let all = scan_all("t.baml", src);
        let params = all
            .filter(
                (b: CodeBlock) -> bool {
                    b.kind == "param"
                },
            );
        assert.equal(params.length(), 0);
    }

    test "match-arm let bindings are not vars" {
        let src = "function f(o: string?) -> string {\n"
            + "    let r = match (o) {\n"
            + "        let v: string => v,\n"
            + "        null => \"\",\n"
            + "    };\n"
            + "    r\n"
            + "}\n";
        let all = scan_all("ma.baml", src);
        let vars = all
            .filter(
                (b: CodeBlock) -> bool {
                    b.kind == "var"
                },
            );
        assert.equal(vars.length(), 1);
        assert.equal(nth_block(vars, 0).name, "r");
    }

    test "lets inside raw-string prompts are not vars" {
        let src = "function Ask(q: string) -> string {\n"
            + "    client: X\n"
            + "    prompt: #\"\n"
            + "        let pretend = code;\n"
            + "        {{ q }}\n"
            + "    \"#\n"
            + "}\n";
        let all = scan_all("r.baml", src);
        let vars = all
            .filter(
                (b: CodeBlock) -> bool {
                    b.kind == "var"
                },
            );
        assert.equal(vars.length(), 0);
    }
}

testset "scanner_tree" {
    test "tree assembly fills sorted child_ids and the program manifest" {
        let blocks = scan_all(
            "t.baml",
            "class A {\n    x: int,\n}\nfunction f(a: A) -> int {\n    let n = 1;\n    n\n}\n",
        );
        // simulate what build_tree does for one file
        let none: string[] = [];
        blocks
            .push(
                CodeBlock {
                    id: "F:t.baml",
                    parent_id: "PROGRAM",
                    kind: "file",
                    name: "t.baml",
                    file: "t.baml",
                    start_line: 1,
                    end_line: 7,
                    source: "",
                    child_ids: none,
                    depth: 1,
                },
            );
        blocks
            .push(
                CodeBlock {
                    id: "PROGRAM",
                    parent_id: null,
                    kind: "program",
                    name: "PROGRAM",
                    file: "",
                    start_line: 0,
                    end_line: 0,
                    source: "t.baml (2 decls, 7 lines)\n",
                    child_ids: none,
                    depth: 0,
                },
            );
        let tree = fill_child_ids(blocks);
        let prog = find_block(tree, "PROGRAM");
        assert.equal(prog.child_ids.length(), 1);
        let file = find_block(tree, "F:t.baml");
        assert.equal(file.child_ids.length(), 2);
        assert.equal(nth_block(tree, 0).id, "PROGRAM"); // depth-first sort order
        let f = find_block(tree, "D:t.baml:f");
        // param + var, in line order
        assert.equal(f.child_ids.length(), 2);
        match (f.child_ids.at(0)) {
            let c: string => assert.equal(c, "M:t.baml:f.a"),
            null => assert.is_true(false),
        }
    }
}
