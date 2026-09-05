enum CorpusLicense {
    PublicDomain,
}

enum CorpusFeature {
    CastlingRights,
    EnPassantTarget,
    TerminalAtDepthOne,
    DuplicateFen,
}

class CorpusSource {
    id: string,
    repository: string,
    commit: string,
    path: string,
    sha256: string,
    license: CorpusLicense,
    position_author: string,
    collector: string,
    public_domain_evidence_urls: string[],
}

class CorpusProfile {
    name: string,
    selected_unique_positions: int,
    assertions: int,
    expected_nodes: int,
}

class CorpusPosition {
    id: string,
    source_line: int,
    raw_epd: string,
    fen: string,
    expected_nodes: int[],
    public_depths: int[],
    duplicate_of: string?,
    features: CorpusFeature[],
}

class PerftCorpus {
    schema_version: int,
    source: CorpusSource,
    public_profile: CorpusProfile,
    positions: CorpusPosition[],
}

class PerftAssertion {
    id: string,
    fen: string,
    depth: int,
    expected_nodes: int,
}

function load_perft_corpus() -> PerftCorpus {
    let corpus = baml.json.from_string<PerftCorpus>(baml.fs.read("testdata/chess-perft-corpus.json"));
    assert.equal(corpus.schema_version, 1);
    corpus
}

function expected_nodes_at(position: CorpusPosition, depth: int) -> int {
    if (depth < 1 || depth > position.expected_nodes.length()) {
        return baml.sys.panic(`missing depth ${depth} for ${position.id}`);
    }
    position.expected_nodes[depth - 1]
}

function public_perft_assertions() -> PerftAssertion[] {
    let assertions: PerftAssertion[] = [];
    for (let position in load_perft_corpus().positions) {
        for (let depth in position.public_depths) {
            assertions.push(
                PerftAssertion {
                    id: `${position.id}_d${depth}`,
                    fen: position.fen,
                    depth,
                    expected_nodes: expected_nodes_at(position, depth),
                },
            );
        }
    }
    assertions
}

testset "public_domain_perft_corpus" {
    for (let c in public_perft_assertions()) {
        test c.id {
            assert.equal(perft(c.fen, c.depth), c.expected_nodes)
        }
    }
}
