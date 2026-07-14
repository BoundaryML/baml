// Keyless streaming smokes -- string-typed and class-typed T.
// Port of llm_functions/customizable/test_streaming_e2e.py: exercises the
// full streaming path (bridge -> BAML LLM client -> HTTP -> SSE ->
// StreamAccumulator -> SAP -> Stream.next()/final() -> bridge) without
// hitting OpenAI: ReplayServer (see replay_harness.h) replays a
// checked-in SSE recording with the env-driven StreamStub client pointed
// at it.
//
//   * string T -- stream_e2e_extract (Stream<null | string, string>)
//   * class  T -- stream_e2e_extract_doc (Stream<StreamingDoc$stream,
//     StreamingDoc>), the regression guard for the class-typed streaming
//     bug.
//
// Re-record the SSE fixtures (needs a real key):
//   INSTA_UPDATE=always infisical run -- cargo nextest run -p
//   sdk_test_llm_recordings
#include <baml_sdk.h>
#include <baml_test.h>

#include <optional>
#include <string>
#include <variant>

#include "replay_harness.h"

using baml_sdk::baml::stream::StreamFinished;
namespace lorem = baml_sdk::lorem;
using StreamingDocPartial = baml_sdk::stream_types::lorem::StreamingDoc;

// ---------------------------------------------------------------------------
// String-typed T -- Stream<null | string, string>.
// ---------------------------------------------------------------------------

BAML_TEST(stream) {
  // Sync next() yields a stream of partials and drains to StreamFinished.
  replay::ReplayServer server("replay_extract_string");
  auto stream = lorem::stream_e2e_extract_stream("ignored-by-replay-server");
  int results = 0;
  while (true) {
    const auto v = stream.next();
    if (std::holds_alternative<StreamFinished>(v)) {
      break;
    }
    ++results;
    BAML_ASSERT(results < 10000);  // stream.next() failed to terminate
  }
  BAML_ASSERT(results >= 10);  // expected at least 10 partials
  const std::string final_value = stream.final();
  BAML_ASSERT(!final_value.empty());
}

BAML_TEST(stream_async) {
  // Async siblings: next_async() / final_async() via baml::Future.
  replay::ReplayServer server("replay_extract_string");
  auto stream =
      lorem::stream_e2e_extract_stream_async("ignored-by-replay-server").get();
  int results = 0;
  while (true) {
    const auto v = stream.next_async().get();
    if (std::holds_alternative<StreamFinished>(v)) {
      break;
    }
    ++results;
    BAML_ASSERT(results < 10000);  // stream.next_async() failed to terminate
  }
  BAML_ASSERT(results >= 10);
  const std::string final_value = stream.final_async().get();
  BAML_ASSERT(!final_value.empty());
}

BAML_TEST(stream_collect_in_baml) {
  // BAML-driven counterpart: the S | StreamFinished union stays
  // engine-side.
  replay::ReplayServer server("replay_extract_string");
  const lorem::StreamE2ECollectResult result =
      lorem::stream_e2e_collect("ignored-by-replay-server");
  BAML_ASSERT(result.next_calls.size() >= 10);
  BAML_ASSERT(!result.final_call.empty());
}

// ---------------------------------------------------------------------------
// Class-typed T -- Stream<StreamingDoc$stream, StreamingDoc>. The case the
// plain-string tests above deliberately avoid.
// ---------------------------------------------------------------------------

BAML_TEST(stream_doc) {
  replay::ReplayServer server("replay_extract_doc");
  auto stream =
      lorem::stream_e2e_extract_doc_stream("ignored-by-replay-server");
  int results = 0;
  bool saw_titled_partial = false;
  while (true) {
    const auto v = stream.next();
    if (std::holds_alternative<StreamFinished>(v)) {
      break;
    }
    ++results;
    const auto& partial = std::get<std::optional<StreamingDocPartial>>(v);
    if (partial.has_value() && partial->title.has_value()) {
      saw_titled_partial = true;
    }
    BAML_ASSERT(results < 10000);  // stream.next() failed to terminate
  }
  BAML_ASSERT(results >= 10);
  BAML_ASSERT(saw_titled_partial);
  const lorem::StreamingDoc final_doc = stream.final();
  BAML_ASSERT(!final_doc.title.empty());
}

BAML_TEST(stream_doc_async) {
  replay::ReplayServer server("replay_extract_doc");
  auto stream =
      lorem::stream_e2e_extract_doc_stream_async("ignored-by-replay-server")
          .get();
  int results = 0;
  while (true) {
    const auto v = stream.next_async().get();
    if (std::holds_alternative<StreamFinished>(v)) {
      break;
    }
    ++results;
    BAML_ASSERT(results < 10000);  // stream.next_async() failed to terminate
  }
  BAML_ASSERT(results >= 10);
  const lorem::StreamingDoc final_doc = stream.final_async().get();
  BAML_ASSERT(!final_doc.title.empty());
}

BAML_TEST(stream_doc_collect_in_baml) {
  // Only the concrete StreamingDoc crosses the FFI boundary.
  replay::ReplayServer server("replay_extract_doc");
  const lorem::StreamingDoc result =
      lorem::stream_e2e_collect_doc("ignored-by-replay-server");
  BAML_ASSERT(!result.title.empty());
}
