# Stream and media actual-ABI probe

This repository-only .NET 10 executable runs in one-program-per-process modes:

- `media` initializes canonical `type_shapes` bytecode and round-trips all four
  media kinds through the actual API table as URL, base64/owned bytes, and
  file/eager-owned bytes, including handle cleanup on decode failure.
- `stream` starts a replay-server/runtime child, then starts a separate
  consumer child with the selected endpoint present before native runtime
  initialization. The consumer drives `ai.stream.Stream.next`/`final` as
  ordinary native calls. A deliberately slow consumer proves one demand gives
  one partial completion and idle time produces no pushed completion. It
  validates the exact typed pull-union descriptor and selected arm, rejects
  contradictory metadata, and requires every boundary-independent ordered
  partial prefix to converge to the pinned canonical final UTF-8 identity.

The fixture uses the same pinned internal Protobuf generation contract as the
protocol probe. It does not expose Protobuf or define the final public
`BamlStream`/media implementation. Builds must select
`BamlNativeProbeMode=Direct` for an explicit native path or `Package` with an
isolated evidence feed; ambiguous implicit package omission is rejected.
