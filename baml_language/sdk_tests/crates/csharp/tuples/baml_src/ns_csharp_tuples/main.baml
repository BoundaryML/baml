// Tuples across the C# boundary. Every tuple of at most 7 slots projects to a
// native `ValueTuple`, including tuples whose slots erase in C# (a literal or
// a nullable reference): `("tag", int)` and `(string, int)` are both
// `(string, long)`, and the declared type states which tuple crosses. Wider
// tuples have no C# type.

class TupleHolder {
  pair (int, string)
  pairs (string, int)[]
  maybe (int, bool)?
  tagged ("tag", int)

  function first(self) -> (int, string) {
    self.pair
  }
}

function echo_pair(value: (int, string)) -> (int, string) {
  value
}

function swap_pair(value: (int, string)) -> (string, int) {
  (value[1], value[0])
}

function echo_single(value: (int,)) -> (int,) {
  value
}

function echo_nested(value: ((int, string), bool)) -> ((int, string), bool) {
  value
}

function echo_seven(
  value: (int, int, int, int, int, int, int),
) -> (int, int, int, int, int, int, int) {
  value
}

function echo_literal(value: ("tag", int)) -> ("tag", int) {
  value
}

function echo_optional_slot(value: (string?, int)) -> (string?, int) {
  value
}

function echo_holder(value: TupleHolder) -> TupleHolder {
  value
}

function echo_union_of_tuples(
  value: (int, bool) | (string, bool),
) -> (int, bool) | (string, bool) {
  value
}

function echo_product_of_unions(value: (int | string, bool)) -> (int | string, bool) {
  value
}

function echo_tuple_map(value: map<string, (int, string)>) -> map<string, (int, string)> {
  value
}

function call_pair_callback(
  callback: ((int, string)) -> (string, int),
  value: (int, string),
) -> (string, int) {
  callback(value)
}

function make_unknown_tuple() -> unknown {
  (1, "one")
}

function echo_unknown(value: unknown) -> unknown {
  value
}

function throw_pair(value: (int, string)) -> int throws (int, string) {
  throw value
}

// Carried tuple identity: an engine tuple carries its instantiated type, so a
// dynamic value finds its C# tuple by that type, not by its slot values.
function make_union_slot_tuple() -> unknown {
  let value: (int | string, bool) = (1, true);
  value
}

// Declared only so their C# types are registered for the fallback stages.
function echo_join_list(value: (int | string)[]) -> (int | string)[] {
  value
}

function echo_wide_join_list(value: (int | string | bool)[]) -> (int | string | bool)[] {
  value
}

function echo_ints(value: int[]) -> int[] {
  value
}

// Tuple types no signature declares: no registered C# tuple, so they take
// the list of the join of their slot types, then the heterogeneous list.
function make_unregistered_tuple() -> unknown {
  let value: (string, int, string) = ("a", 2, "c");
  value
}

function make_repeated_tuple() -> unknown {
  let value: (int, int, int) = (1, 2, 3);
  value
}

function make_union_slot_triple() -> unknown {
  let value: (int | string, bool, string) = ("x", false, "y");
  value
}

function make_unjoined_tuple() -> unknown {
  let value: (int, string, float) = (1, "b", 2.5);
  value
}

// Dynamic tuples whose C# type erases or does not exist: a literal slot
// erases, so only the BAML value names it; a nullable-reference slot shares
// its C# type with the non-null tuple; eight slots have no C# tuple.
function make_literal_tuple() -> unknown {
  let value: ("tag", int) = ("tag", 32);
  value
}

function make_optional_slot_tuple() -> unknown {
  let value: (string?, int) = (null, 33);
  value
}

function make_wide_tuple() -> unknown {
  let value: (int, int, int, int, int, int, int, int) = (1, 2, 3, 4, 5, 6, 7, 8);
  value
}

function pair_of<T>(value: T) -> (T, T) {
  (value, value)
}

// A generic tuple carries its substituted slot types.
function make_generic_pair() -> unknown {
  pair_of(21)
}

// Generic tuples, built from their slots once a call binds `T`: `(T, T)` and
// `(T, "tag")` are native tuples, the latter `(T, string)`, stated as the
// declared tuple so the engine checks the literal slot.
function generic_tagged<T>(value: (T, "tag")) -> (T, "tag") {
  value
}

function generic_tagged_nested<T>(value: ((T, "tag"), int)) -> ((T, "tag"), int) {
  value
}

function call_unknown_callback(callback: (unknown) -> unknown) -> unknown {
  callback((22, "twenty-two"))
}

function echo_int_pair(value: (int, int)) -> (int, int) {
  value
}

// Tuples as stream items, with no live LLM or replay server: each function's
// client is a scripted `StreamingClient` that replays fixed text deltas
// through `ai.stream.TurnStream.from_chunks`, so the generated stream
// companion drives the ordinary `Fn@stream` path offline. Mirrors the shared
// `llm_functions/ns_tuple_streams` fixture the other SDKs run.
//
//   * `stream_tuple_items` -- `(string, int)[]`: each partial is a list of
//     complete tuples (`[("a", 1)]`, then `[("a", 1), ("b", 2)]`).
//   * `stream_tuple_final` -- `(int, string)`: each partial is itself a
//     tuple (`(1, "")`, then `(1, "a")`), and so is the final value.
class TupleChunkClient {
  chunks: string[],

  implements ai.Client {
    function requested_model(self) -> string {
      "tuple-chunks"
    }

    function provider(self) -> string {
      "test"
    }

    function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
      let _ = input;
      baml.http.Request {
        method: "POST",
        url: "https://example.invalid/tuple-chunks",
        headers: {},
        body: "{}",
      }
    }

    function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
      let _ = input;
      ai.ModelTurn {
        child_turns: [],
        calls: [],
        content: [ai.content.Text { text: self.chunks.join("") }],
        stop_reason: ai.content.StopReason.Complete,
      }
    }
  }

  implements ai.stream.StreamingClient {
    function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
      let _ = input;
      ai.stream.TurnStream.from_chunks(self.chunks)
    }
  }
}

client TupleItemsChunks = TupleChunkClient {
  chunks: ["[[\"a\", 1]", ", [\"b\"", ", 2]", "]"],
};

client TupleFinalChunks = TupleChunkClient {
  chunks: ["[1, ", "\"a\"]"],
};

function stream_tuple_items(text: string) -> (string, int)[] {
  client: TupleItemsChunks
  prompt: `List the pairs in ${text}. ${ctx.output_format()}`
}

function stream_tuple_final(text: string) -> (int, string) {
  client: TupleFinalChunks
  prompt: `The pair in ${text}. ${ctx.output_format()}`
}
