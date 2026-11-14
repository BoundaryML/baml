// Tuples as stream items, with no live LLM or replay server: each function's
// client is a scripted `StreamingClient` that replays fixed text deltas
// through `ai.stream.TurnStream.from_chunks`, so every SDK's generated stream
// companion drives the ordinary `Fn@stream` path (partial parse per delta,
// final parse at EOF) offline and deterministically.
//
//   * `stream_tuple_items` -- `(string, int)[]`: each partial is a list of
//     complete tuples (`[("a", 1)]`, then `[("a", 1), ("b", 2)]`), the final
//     value the full list.
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

test "stream_tuple_items_yields_tuple_lists" {
  let stream = stream_tuple_items@stream("x");
  let partials: (string, int)[][] = [];
  while let partial: (string, int)[] = stream.next() {
    partials.push(partial);
  }
  assert.equal(partials.length() > 0, true);
  assert.equal(partials[0][0], ("a", 1));
  assert.equal(stream.final(), [("a", 1), ("b", 2)]);
}

test "stream_tuple_final_yields_tuples" {
  let stream = stream_tuple_final@stream("x");
  let last: (int, string)? = null;
  while let partial: (int, string) = stream.next() {
    last = partial;
  }
  assert.equal(last, (1, "a"));
  assert.equal(stream.final(), (1, "a"));
}
