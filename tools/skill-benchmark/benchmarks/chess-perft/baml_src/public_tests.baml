function rejects_perft_input(fen: string, depth: int) -> bool {
    let _ = perft(fen, depth) catch (e) {
        baml.errors.ParseError => {
            return true;
        }
    };
    false
}

test "depth_zero_is_one" {
    assert.equal(perft("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 0), 1)
}

test "rejects_negative_depth" {
    assert.is_true(
        rejects_perft_input("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", -1),
    )
}

test "rejects_malformed_fen" {
    assert.is_true(rejects_perft_input("not a fen", 1))
}
