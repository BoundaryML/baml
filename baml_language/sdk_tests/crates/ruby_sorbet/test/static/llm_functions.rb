# typed: strict
require "sorbet-runtime"
require_relative "../../llm_functions/generated/baml_sdk"

doc = BamlSdk::Lorem::StreamingDoc.new(title: "Title", body: nil, word_count: 30)
T.let(doc.title, String)
T.let(doc.body, T.nilable(String))
T.let(doc.word_count, T.nilable(Integer))
T.let(BamlSdk::Lorem::StreamingDoc.new(title: "Partial").word_count, T.nilable(Integer))
collected = BamlSdk::Lorem::StreamE2ECollectResult.new(next_calls: ["first"], final_call: "last")
T.let(collected.next_calls, T::Array[String])
T.let(BamlSdk::Lorem.stream_e2e_collect("text"), BamlSdk::Lorem::StreamE2ECollectResult)
T.let(BamlSdk::Lorem.streaming_extract("text"), BamlSdk::Lorem::StreamingDoc)
T.let(BamlSdk::Ipsum::Sentiment::POSITIVE, BamlSdk::Ipsum::Sentiment)
T.let(BamlSdk::Ipsum.classify_sentiment("text"), BamlSdk::Ipsum::Sentiment)
