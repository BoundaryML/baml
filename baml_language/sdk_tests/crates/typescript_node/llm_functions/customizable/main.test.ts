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
import * as replay from "./baml_sdk/replay/index.js";
import { Resume, StreamingDoc } from "./baml_sdk/lorem/index.js";
import { Sentiment } from "./baml_sdk/ipsum/index.js";

describe("llm_functions — namespace imports", () => {
  it("test_root_imports_cleanly", () => {
    expect(bamlSdk).toBeDefined();
  });

  it("test_namespaces_reachable_via_explicit_import", () => {
    expect(lorem).toBeDefined();
    expect(ipsum).toBeDefined();
  });
});

describe("llm_functions — class shapes", () => {
  it("test_lorem_resume_class_shape", () => {
    expect(Resume).toBeDefined();
    // Field-set assertion deferred — the python suite uses
    // `pydantic.BaseModel.model_fields`; the TS shape will be a
    // generated interface and is asserted via tsc instead.
  });

  it("test_lorem_streaming_doc_class_shape", () => {
    expect(StreamingDoc).toBeDefined();
  });

  it("test_ipsum_sentiment_enum_shape", () => {
    const members = Object.keys(Sentiment).filter((k) => isNaN(Number(k)));
    expect(new Set(members)).toEqual(
      new Set(["POSITIVE", "NEGATIVE", "NEUTRAL"]),
    );
  });
});

describe("llm_functions — factory + companion bindings", () => {
  it("test_extract_resume_factory_bindings", () => {
    expect(typeof lorem.ExtractResume).toBe("function");
    expect(typeof lorem.ExtractResume_async).toBe("function");
  });

  it("test_extract_resume_companion_bindings", () => {
    expect(typeof lorem.ExtractResume$build_request).toBe("function");
    expect(typeof lorem.ExtractResume$build_request_async).toBe("function");
    expect(typeof lorem.ExtractResume$render_prompt).toBe("function");
    expect(typeof lorem.ExtractResume$render_prompt_async).toBe("function");
    expect(typeof lorem.ExtractResume$parse).toBe("function");
    expect(typeof lorem.ExtractResume$parse_async).toBe("function");
    expect(typeof lorem.ExtractResume$parse_stream).toBe("function");
    expect(typeof lorem.ExtractResume$parse_stream_async).toBe("function");
  });

  it("test_streaming_extract_factory_bindings", () => {
    expect(typeof lorem.StreamingExtract).toBe("function");
    expect(typeof lorem.StreamingExtract_async).toBe("function");
  });

  it("test_streaming_extract_companion_bindings", () => {
    expect(typeof lorem.StreamingExtract$build_request).toBe("function");
    expect(typeof lorem.StreamingExtract$build_request_async).toBe("function");
    expect(typeof lorem.StreamingExtract$render_prompt).toBe("function");
    expect(typeof lorem.StreamingExtract$render_prompt_async).toBe("function");
    expect(typeof lorem.StreamingExtract$parse).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_async).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_stream).toBe("function");
    expect(typeof lorem.StreamingExtract$parse_stream_async).toBe("function");
  });

  it("test_classify_sentiment_factory_bindings", () => {
    expect(typeof ipsum.ClassifySentiment).toBe("function");
    expect(typeof ipsum.ClassifySentiment_async).toBe("function");
  });
});

describe("llm_functions — stream companion classes in lorem leaf", () => {
  it("test_stream_types_lorem_leaf_present", () => {
    const hasAny = ["Resume$stream", "StreamingDoc$stream"].some(
      (name) => name in lorem,
    );
    expect(hasAny).toBe(true);
  });
});

describe("llm_functions — replay namespace", () => {
  it("test_replay_server_namespace_bindings", () => {
    expect(typeof replay.replay_serve_until_shutdown).toBe("function");
    expect(typeof replay.replay_serve_until_shutdown_async).toBe("function");
    expect(typeof replay.replay_serve_detached).toBe("function");
    expect(typeof replay.replay_serve_detached_async).toBe("function");
  });
});

function lowerCaseHeaders(headers: Record<string, string>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]),
  );
}

describe("llm_functions — shorthand client API keys", () => {
  it("test_extract_resume_build_request_includes_openai_api_key", () => {
    const previous = process.env.OPENAI_API_KEY;
    process.env.OPENAI_API_KEY = "sk-openai-shorthand-test";
    try {
      const request = lorem.ExtractResume$build_request("Some resume text");
      expect(lowerCaseHeaders(request.headers).authorization).toBe(
        "Bearer sk-openai-shorthand-test",
      );
    } finally {
      if (previous === undefined) delete process.env.OPENAI_API_KEY;
      else process.env.OPENAI_API_KEY = previous;
    }
  });

  it("test_streaming_extract_build_request_includes_openai_api_key", () => {
    const previous = process.env.OPENAI_API_KEY;
    process.env.OPENAI_API_KEY = "sk-openai-responses-test";
    try {
      const request = lorem.StreamingExtract$build_request("Some text to summarize");
      expect(lowerCaseHeaders(request.headers).authorization).toBe(
        "Bearer sk-openai-responses-test",
      );
    } finally {
      if (previous === undefined) delete process.env.OPENAI_API_KEY;
      else process.env.OPENAI_API_KEY = previous;
    }
  });

  it("test_classify_sentiment_build_request_includes_anthropic_api_key", () => {
    const previous = process.env.ANTHROPIC_API_KEY;
    process.env.ANTHROPIC_API_KEY = "sk-ant-shorthand-test";
    try {
      const request = ipsum.ClassifySentiment$build_request("I love this!");
      expect(lowerCaseHeaders(request.headers)["x-api-key"]).toBe(
        "sk-ant-shorthand-test",
      );
    } finally {
      if (previous === undefined) delete process.env.ANTHROPIC_API_KEY;
      else process.env.ANTHROPIC_API_KEY = previous;
    }
  });
});
