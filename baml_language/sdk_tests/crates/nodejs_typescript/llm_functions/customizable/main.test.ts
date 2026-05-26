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
// - stream_types/lorem leaf presence
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
import * as streamLorem from "./baml_sdk/stream_types/lorem";

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
    expect(typeof (lorem as any).ExtractResume).toBe("function");
    expect(typeof (lorem as any).ExtractResume_async).toBe("function");
  });

  it("lorem.ExtractResume companion bindings exist", () => {
    for (const name of [
      "ExtractResume__build_request",
      "ExtractResume__render_prompt",
      "ExtractResume__parse",
      "ExtractResume__parse_stream",
    ]) {
      expect(typeof (lorem as any)[name]).toBe("function");
      expect(typeof (lorem as any)[`${name}_async`]).toBe("function");
    }
  });

  it("lorem.StreamingExtract sync + async factories are callable", () => {
    expect(typeof (lorem as any).StreamingExtract).toBe("function");
    expect(typeof (lorem as any).StreamingExtract_async).toBe("function");
  });

  it("lorem.StreamingExtract companion bindings exist", () => {
    for (const name of [
      "StreamingExtract__build_request",
      "StreamingExtract__render_prompt",
      "StreamingExtract__parse",
      "StreamingExtract__parse_stream",
    ]) {
      expect(typeof (lorem as any)[name]).toBe("function");
      expect(typeof (lorem as any)[`${name}_async`]).toBe("function");
    }
  });

  it("ipsum.ClassifySentiment sync + async factories are callable", () => {
    expect(typeof (ipsum as any).ClassifySentiment).toBe("function");
    expect(typeof (ipsum as any).ClassifySentiment_async).toBe("function");
  });
});

describe("llm_functions — stream_types leaf", () => {
  it("stream_types/lorem exposes at least one $stream companion class", () => {
    const hasAny = ["Resume", "StreamingDoc"].some(
      (name) => name in streamLorem,
    );
    expect(hasAny).toBe(true);
  });
});
