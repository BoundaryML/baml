
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

function make_union_slot_tuple() -> unknown {
  let value: (int | string, bool) = (1, true);
  value
}

function echo_join_list(value: (int | string)[]) -> (int | string)[] {
  value
}

function echo_wide_join_list(value: (int | string | bool)[]) -> (int | string | bool)[] {
  value
}

function echo_ints(value: int[]) -> int[] {
  value
}

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

function make_generic_pair() -> unknown {
  pair_of(21)
}

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
