// Stage (a2) — tree assembly. scan_file_decls/members_of produce blocks with
// parent_id set; this file adds file blocks and the PROGRAM superblock, fills
// child_ids, and returns the whole tree deterministically sorted.

// Build the full CodeBlock tree for a target project. Sorted by
// (depth, file, start_line, id); child_ids sorted by (start_line, id).
function build_tree(root: string) -> CodeBlock[] {
    let all: CodeBlock[] = [];
    let manifest = "";
    for (let rel in list_baml_files(root)) {
        let src = baml.fs.read(root + "/" + rel) catch (_) {
            _ => ""
        };
        if (src != "") {
            let decls = scan_file_decls(rel, src);
            for (let d in decls) {
                all.push(d);
                for (let m in members_of(d)) {
                    all.push(m);
                }
            }
            let loc = src.lines().length();
            let none: string[] = [];
            all.push(
                CodeBlock {
                    id: "F:" + rel,
                    parent_id: "PROGRAM",
                    kind: "file",
                    name: rel,
                    file: rel,
                    start_line: 1,
                    end_line: loc,
                    source: src,
                    child_ids: none,
                    depth: 1,
                },
            );
            manifest = manifest
                + rel
                + " ("
                + i2s(decls.length())
                + " decls, "
                + i2s(loc)
                + " lines)\n";
        }
    }
    let none: string[] = [];
    all.push(
        CodeBlock {
            id: "PROGRAM",
            parent_id: null,
            kind: "program",
            name: "PROGRAM",
            file: "",
            start_line: 0,
            end_line: 0,
            source: manifest,
            child_ids: none,
            depth: 0,
        },
    );
    fill_child_ids(all)
}

// Rebuild every block with its child_ids filled (children sorted by
// (start_line, id)), and the whole list sorted by (depth, file, start_line, id).
function fill_child_ids(blocks: CodeBlock[]) -> CodeBlock[] {
    // parent_id -> sort keys of children
    let kids: map<string, string[]> = {};
    for (let b in blocks) {
        match (b.parent_id) {
            let p: string => {
                let ks: string[] = match (kids.get(p)) {
                    let v: string[] => v,
                    null => {
                        let empty: string[] = [];
                        empty
                    },
                };
                ks.push(pad6(b.start_line) + "\t" + b.id);
                kids.set(p, ks);
            },
            null => null,
        }
    }
    let out: CodeBlock[] = [];
    for (let b in blocks) {
        let child_ids: string[] = [];
        match (kids.get(b.id)) {
            let ks: string[] => {
                for (
                    let k in ks
                        .sort_by_key(
                            (s: string) -> string {
                                s
                            },
                        )
                ) {
                    let sep = k.index_of("\t");
                    child_ids.push(k.substring(sep + 1, k.length()));
                }
            },
            null => null,
        }
        out.push(
            CodeBlock {
                id: b.id,
                parent_id: b.parent_id,
                kind: b.kind,
                name: b.name,
                file: b.file,
                start_line: b.start_line,
                end_line: b.end_line,
                source: b.source,
                child_ids: child_ids,
                depth: b.depth,
            },
        );
    }
    out.sort_by_key(
        (b: CodeBlock) -> string {
            i2s(b.depth) + "\t" + b.file + "\t" + pad6(b.start_line) + "\t" + b.id
        },
    )
}

// id -> block index for downstream stages.
function block_index(blocks: CodeBlock[]) -> map<string, CodeBlock> {
    let by_id: map<string, CodeBlock> = {};
    for (let b in blocks) {
        by_id.set(b.id, b);
    }
    by_id
}

// First non-empty source line of a block, for compact prompt tables.
function block_head(b: CodeBlock) -> string {
    for (let line in b.source.lines()) {
        if (line.trim() != "") {
            return line.trim();
        }
    }
    ""
}
