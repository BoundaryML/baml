// Stage (b) — the interaction table. Deterministic identifier scan: chunk A
// gets an edge to chunk B when B's declared name appears as an identifier in
// A's source. Edge kinds:
//   test chunk  -> anything     : "exercises"
//   anything    -> type chunk   : "uses_type"
//   otherwise                    : "calls"
function build_edges(chunks: Chunk[]) -> Edge[] {
    // name -> ids of chunks declaring that name (name collisions across files
    // produce edges to every declarer; deterministic because chunks are sorted).
    let by_name: map<string, string[]> = {};
    let kind_of: map<string, string> = {};
    for (let c in chunks) {
        let ids: string[] = match (by_name.get(c.name)) {
            let v: string[] => v,
            null => {
                let empty: string[] = [];
                empty
            },
        };
        ids.push(c.id);
        by_name.set(c.name, ids);
        kind_of.set(c.id, c.kind);
    }
    let edges: Edge[] = [];
    for (let c in chunks) {
        for (let ident in identifiers(c.source)) {
            if (ident == c.name) {
                // self-reference; skip
            } else {
                match (by_name.get(ident)) {
                    let targets: string[] => {
                        for (let to in targets) {
                            let to_kind = match (kind_of.get(to)) {
                                let k: string => k,
                                null => "",
                            };
                            let kind = if (c.kind == "test") {
                                "exercises"
                            } else {
                                if (to_kind == "type") {
                                    "uses_type"
                                } else {
                                    "calls"
                                }
                            };
                            edges.push(Edge { from_id: c.id, to_id: to, kind: kind });
                        }
                    },
                    null => null,
                }
            }
        }
    }
    edges
        .sort_by_key(
            (e: Edge) -> string {
                e.from_id + "->" + e.to_id
            },
        )
}

// Up to `limit` one-line descriptions of a chunk's 1-hop neighborhood, for
// grader context. Deterministic: edges are pre-sorted.
function neighbor_context(chunk_id: string, edges: Edge[], chunks: Chunk[], limit: int) -> string {
    let first_line: map<string, string> = {};
    for (let c in chunks) {
        let head = "";
        for (let line in c.source.lines()) {
            if (head == "" && line.trim() != "") {
                head = line.trim();
            }
        }
        first_line.set(c.id, c.kind + " | " + head);
    }
    let out = "";
    let n = 0;
    for (let e in edges) {
        if (n < limit && (e.from_id == chunk_id || e.to_id == chunk_id)) {
            let other = if (e.from_id == chunk_id) {
                e.to_id
            } else {
                e.from_id
            };
            let dir = if (e.from_id == chunk_id) {
                "-> "
            } else {
                "<- "
            };
            let desc = match (first_line.get(other)) {
                let d: string => d,
                null => "",
            };
            out = out + dir + e.kind + " " + other + " (" + desc + ")\n";
            n = n + 1;
        }
    }
    out
}
