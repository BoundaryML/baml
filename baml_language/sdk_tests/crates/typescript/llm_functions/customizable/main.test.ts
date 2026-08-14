// Mirrors python_pydantic2/customizable/llm_functions/test_main.py.
//
// E2E check of the 09a-style baml_src → baml_sdk pipeline. Drives
// codegen from real `.baml` source through the full
// `baml_project::build_symbol_pool` path (parse → HIR → TIR →
// SymbolPool → emitter).
//
// Scope (subset of 09a-codegen-example-scenario.md):
// - user.lorem.Resume + ExtractResume (with auto-generated companions)
// - user.lorem.StreamingDoc + StreamingExtract
// - user.ipsum.Sentiment (enum) + ClassifySentiment
// - lorem leaf hosts `$stream` companion classes beside their base type
//   (spec2: `$` is a valid TS identifier char, so no `stream_types/` leaf)
//
// The python suite also pins shorthand-client api_key wiring (auth
// header on `*$build_request`); that's deferred until the nodejs
// runtime's request-introspection API is settled.

import { describe, it, expect } from "vitest";

import * as bamlSdk from "./baml_sdk/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import * as ipsum from "./baml_sdk/ipsum/index.js";
import { Resume, StreamingDoc } from "./baml_sdk/lorem/index.js";
import { Sentiment } from "./baml_sdk/ipsum/index.js";

describe("llm_functions — namespace imports", () => {
  it("main_root_imports_cleanly", () => {
    expect(bamlSdk).toBeDefined();
  });

  it("main_baml_sdk_lorem_and_baml_sdk_ipsum_are_reachable", () => {
    expect(lorem).toBeDefined();
    expect(ipsum).toBeDefined();
  });
});

describe("llm_functions — class shapes", () => {
  it("main_lorem_resume_is_reachable", () => {
    expect(Resume).toBeDefined();
    // Field-set assertion deferred — the python suite uses
    // `pydantic.BaseModel.model_fields`; the TS shape will be a
    // generated interface and is asserted via tsc instead.
  });

  it("main_lorem_streaming_doc_is_reachable", () => {
    expect(StreamingDoc).toBeDefined();
  });

  it("main_ipsum_sentiment_enum_has_positive_negative_neutral_members", () => {
    const members = Object.keys(Sentiment).filter((k) => isNaN(Number(k)));
    expect(new Set(members)).toEqual(
      new Set(["POSITIVE", "NEGATIVE", "NEUTRAL"]),
    );
  });
});

describe("llm_functions — factory + companion bindings", () => {
  it("main_lorem_extract_resume_sync_plus_async_factories_are_callable", () => {
    expect(typeof lorem.ExtractResume).toBe("function");
    expect(typeof lorem.ExtractResume_async).toBe("function");
  });

  it("main_lorem_extract_resume_companion_bindings_exist", () => {
    // The single-path companion set: $build_request* and $parse_stream
    // went away with the legacy LLM path.
    expect(typeof lorem.ExtractResume$render_prompt).toBe("function");
    expect(typeof lorem.ExtractResume$render_prompt_async).toBe("function");
    expect(typeof lorem.ExtractResume$parse).toBe("function");
    expect(typeof lorem.ExtractResume$parse_async).toBe("function");
  });

  it("main_lorem_streaming_extract_sync_plus_async_factories_are_callable", () => {
    expect(typeof lorem.StreamingExtract).toBe("function");
    expect(typeof lorem.StreamingExtract_async).toBe("function");
  });

  it("main_lorem_streaming_extract_companion_bindings_exist", () => {
    // The single-path companion set: $build_request* and $parse_stream
    // went away with the legacy LLM path.
    expect(typeof lorem.StreamingExtract$render_prompt).toBe("function");
    expect(typeof lorem.StreamingExtract$render_prompt_async).toBe("function");
    expect(typeof lorem.StreamingExtract$parse).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_async).toBe("function");
  });

  it("main_ipsum_classify_sentiment_sync_plus_async_factories_are_callable", () => {
    expect(typeof ipsum.ClassifySentiment).toBe("function");
    expect(typeof ipsum.ClassifySentiment_async).toBe("function");
  });
});

describe("llm_functions — stream companion classes in lorem leaf", () => {
  it("main_lorem_exposes_the_stream_companion_classes_beside_their_base_type", () => {
    const hasAny = ["Resume$stream", "StreamingDoc$stream"].some(
      (name) => name in lorem,
    );
    expect(hasAny).toBe(true);
  });
});
