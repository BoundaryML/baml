# Chess perft specification

Implement one function:

```baml
function perft(
    fen: string,
    depth: int,
) -> int throws baml.errors.ParseError
```

## Result

`perft` counts the legal leaf positions reachable from `fen` in exactly `depth`
plies.

- `perft(fen, 0)` is `1` for every valid position.
- At depth greater than zero, return the sum of `perft(child, depth - 1)` for
  every legal move from the position.
- A position with no legal moves returns `0` at every positive depth.
- Checkmate, stalemate, repetition, the fifty-move rule, insufficient material,
  and other draw adjudication do not terminate the traversal early.
- Reject a negative depth with `baml.errors.ParseError` and a nonempty message.

The implementation only needs to support standard chess. Chess960 and variant
rules are out of scope.

## FEN input

Accept a standard six-field Forsyth-Edwards Notation string:

1. Piece placement
2. Active color: `w` or `b`
3. Castling availability: `K`, `Q`, `k`, `q`, or `-`
4. En passant target square or `-`
5. Halfmove clock
6. Fullmove number

Piece placement has eight slash-separated ranks from rank 8 through rank 1.
Digits expand to empty squares. The recognized pieces are `KQRBNP` and
`kqrbnp`.

Reject malformed FEN with `baml.errors.ParseError` and a nonempty message. This
includes the wrong number of fields or ranks, a rank that does not expand to
eight squares, unknown pieces, an invalid active color, malformed or duplicate
castling flags, an invalid en passant square, a negative halfmove clock, or a
fullmove number below one.

The supplied corpus contains structurally valid positions. The implementation
may reject additional impossible positions, but it must accept every supplied
corpus position.

## Legal moves

Generate every legal move for the active color, including:

- Pawn single and double pushes
- Pawn captures and en passant captures
- Promotion to queen, rook, bishop, or knight, for pushes and captures
- Knight, bishop, rook, queen, and king moves
- Kingside and queenside castling

A legal move must not leave the moving side's king in check. Sliding pieces stop
at the first occupied square. A king may not move onto an attacked square.

Castling is legal only when the corresponding FEN right is present, the king and
rook occupy their standard starting squares, the squares between them are empty,
and the king is not in check and does not cross or land on an attacked square.

An en passant capture is legal only when the FEN target and board position permit
it and the capture does not leave the moving side's king in check.

Update castling rights after a king move, a rook move from its starting square,
or the capture of a rook on its starting square. Set an en passant target only
after a pawn double push. The move counters do not affect perft results, but
successor positions must otherwise follow standard chess rules.

## Integer range

The public profile reaches depth 4 and stays within BAML's `int` range. The full
JSON corpus includes reference counts through depth 6 for optional local testing.
