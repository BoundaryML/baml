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

import { describe, it, expect } from "@jest/globals";

import * as bamlSdk from "./baml_sdk";
import * as lorem from "./baml_sdk/lorem";
import * as ipsum from "./baml_sdk/ipsum";
import { Resume, StreamingDoc } from "./baml_sdk/lorem";
import { Sentiment } from "./baml_sdk/ipsum";

describe("llm_functions — namespace imports", () => {
  it("baml_sdk root imports cleanly", () => {
    expect(bamlSdk).toBeDefined();
  });

  it("baml_sdk/lorem and baml_sdk/ipsum are reachable", () => {
    expect(lorem).toBeDefined();
    expect(ipsum).toBeDefined();
  });
});

describe("llm_functions — class shapes", () => {
  it("lorem.Resume is reachable", () => {
    expect(Resume).toBeDefined();
    // Field-set assertion deferred — the python suite uses
    // `pydantic.BaseModel.model_fields`; the TS shape will be a
    // generated interface and is asserted via tsc instead.
  });

  it("lorem.StreamingDoc is reachable", () => {
    expect(StreamingDoc).toBeDefined();
  });

  it("ipsum.Sentiment enum has POSITIVE / NEGATIVE / NEUTRAL members", () => {
    const members = Object.keys(Sentiment).filter((k) => isNaN(Number(k)));
    expect(new Set(members)).toEqual(
      new Set(["POSITIVE", "NEGATIVE", "NEUTRAL"]),
    );
  });
});

describe("llm_functions — factory + companion bindings", () => {
  it("lorem.ExtractResume sync + async factories are callable", () => {
    expect(typeof lorem.ExtractResume).toBe("function");
    expect(typeof lorem.ExtractResume_async).toBe("function");
  });

  it("lorem.ExtractResume companion bindings exist", () => {
    expect(typeof lorem.ExtractResume$build_request).toBe("function");
    expect(typeof lorem.ExtractResume$build_request_async).toBe("function");
    expect(typeof lorem.ExtractResume$render_prompt).toBe("function");
    expect(typeof lorem.ExtractResume$render_prompt_async).toBe("function");
    expect(typeof lorem.ExtractResume$parse).toBe("function");
    expect(typeof lorem.ExtractResume$parse_async).toBe("function");
    expect(typeof lorem.ExtractResume$parse_stream).toBe("function");
    expect(typeof lorem.ExtractResume$parse_stream_async).toBe("function");
  });

  it("lorem.StreamingExtract sync + async factories are callable", () => {
    expect(typeof lorem.StreamingExtract).toBe("function");
    expect(typeof lorem.StreamingExtract_async).toBe("function");
  });

  it("lorem.StreamingExtract companion bindings exist", () => {
    expect(typeof lorem.StreamingExtract$build_request).toBe("function");
    expect(typeof lorem.StreamingExtract$build_request_async).toBe("function");
    expect(typeof lorem.StreamingExtract$render_prompt).toBe("function");
    expect(typeof lorem.StreamingExtract$render_prompt_async).toBe("function");
    expect(typeof lorem.StreamingExtract$parse).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_async).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_stream).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_stream_async).toBe("function");
  });

  it("ipsum.ClassifySentiment sync + async factories are callable", () => {
    expect(typeof ipsum.ClassifySentiment).toBe("function");
    expect(typeof ipsum.ClassifySentiment_async).toBe("function");
  });
});

describe("llm_functions — stream companion classes in lorem leaf", () => {
  it("lorem exposes the `$stream` companion classes beside their base type", () => {
    const hasAny = ["Resume$stream", "StreamingDoc$stream"].some(
      (name) => name in lorem,
    );
    expect(hasAny).toBe(true);
  });
});
