# Chess perft BAML benchmark

This benchmark asks an agent to implement standard-chess legal move generation
behind one interface:

```baml
function perft(fen: string, depth: int) -> int throws baml.errors.ParseError
```

The public test profile is generated from Marcel van Kervinck's public-domain
perft positions. The checked-in JSON preserves all 6,838 source rows and all
reference counts from depths 1 through 6. A bounded, deterministic subset is
enabled as native BAML tests so the default suite remains practical.

See `SPEC.md` for behavior and `THIRD_PARTY_NOTICES.md` for provenance.

```bash
baml test --list
baml test
```
