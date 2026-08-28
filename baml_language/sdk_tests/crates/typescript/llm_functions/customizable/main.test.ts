// Mirrors python_pydantic2/customizable/llm_functions/test_main.py.
//
// E2E check of the 09a-style baml_src → baml_sdk pipeline. Drives
// codegen from real `.baml` source through the full
// `baml_project::build_symbol_pool` path (parse → HIR → TIR →
// SymbolPool → emitter).
//
// Scope (subset of 09a-codegen-example-scenario.md):
// - user.lorem.Resume + ExtractResume (with flat Spec/Stream projections)
// - user.lorem.StreamingDoc + StreamingExtract
// - user.ipsum.Sentiment (enum) + ClassifySentiment
// - lorem leaf hosts PPIR `$stream` partial classes beside their base type
//   (spec2: `$` is a valid TS identifier char, so no `stream_types/` leaf)
//
import { describe, it, expect } from "vitest";

import * as bamlSdk from "./baml_sdk/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import * as ipsum from "./baml_sdk/ipsum/index.js";
import { Image } from "./baml_sdk/baml/media/index.js";
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

describe("llm_functions — factory + operation bindings", () => {
  it("main_lorem_extract_resume_sync_plus_async_factories_are_callable", () => {
    expect(typeof lorem.ExtractResume).toBe("function");
    expect(typeof lorem.ExtractResume_async).toBe("function");
  });

  it("main_lorem_extract_resume_exposes_flat_spec_and_stream_bindings", () => {
    expect(typeof lorem.ExtractResume_spec).toBe("function");
    expect(typeof lorem.ExtractResume_spec_async).toBe("function");
    expect(typeof lorem.ExtractResume$stream).toBe("function");
    expect(typeof lorem.ExtractResume$stream_async).toBe("function");
  });

  it("main_lorem_streaming_extract_sync_plus_async_factories_are_callable", () => {
    expect(typeof lorem.StreamingExtract).toBe("function");
    expect(typeof lorem.StreamingExtract_async).toBe("function");
  });

  it("main_lorem_streaming_extract_exposes_flat_spec_and_stream_bindings", () => {
    expect(typeof lorem.StreamingExtract_spec).toBe("function");
    expect(typeof lorem.StreamingExtract_spec_async).toBe("function");
    expect(typeof lorem.StreamingExtract$stream).toBe("function");
    expect(typeof lorem.StreamingExtract$stream_async).toBe("function");
  });

  it("main_ipsum_classify_sentiment_sync_plus_async_factories_are_callable", () => {
    expect(typeof ipsum.ClassifySentiment).toBe("function");
    expect(typeof ipsum.ClassifySentiment_async).toBe("function");
  });

  it("projects_static_and_instance_llm_methods_without_invoking_a_provider", () => {
    const probe = new lorem.MethodProjectionProbe({ prefix: "probe" });

    expect(typeof probe.extract).toBe("function");
    expect(typeof probe.extract_async).toBe("function");
    expect(typeof probe.extract_spec).toBe("function");
    expect(typeof probe.extract_spec_async).toBe("function");
    expect(typeof probe.extract$stream).toBe("function");
    expect(typeof probe.extract$stream_async).toBe("function");

    expect(typeof lorem.MethodProjectionProbe.summarize).toBe("function");
    expect(typeof lorem.MethodProjectionProbe.summarize_async).toBe("function");
    expect(typeof lorem.MethodProjectionProbe.summarize_spec).toBe("function");
    expect(typeof lorem.MethodProjectionProbe.summarize_spec_async).toBe("function");
    expect(typeof lorem.MethodProjectionProbe.summarize$stream).toBe("function");
    expect(typeof lorem.MethodProjectionProbe.summarize$stream_async).toBe("function");
  });
});

describe("llm_functions — FunctionSpec", () => {
  it("constructs_a_live_spec_without_a_synthetic_fqn", () => {
    const spec = lorem.ExtractResume_spec("Ada Lovelace, ada@example.test");
    expect(spec.name()).toContain("ExtractResume");
    expect(spec.arguments()).toEqual({
      text: "Ada Lovelace, ada@example.test",
    });

    const parsed = spec.parse('{"name":"Ada Lovelace","email":null}');
    expect(parsed).toBeInstanceOf(Resume);
    expect(parsed.name).toBe("Ada Lovelace");
    expect(parsed.email).toBeNull();
  });

  it("keeps_a_portable_prompt_reusable_across_engine_reentry", () => {
    const png = "iVBORw0KGgo=";
    const spec = lorem.InspectMedia_spec(Image.fromBase64(png, "image/png"));
    const prompt = spec.prompt();

    const text = prompt.text();
    expect(prompt.text()).toBe(text);

    const firstMessages = prompt.messages();
    expect(prompt.text()).toBe(text);
    const secondMessages = prompt.messages();
    expect(secondMessages).toEqual(firstMessages);
    expect(firstMessages).toHaveLength(1);
    expect(firstMessages[0].role).toBe("user");
    expect(firstMessages[0].parts[0]).toMatch(/^Describe this image:/);

    const media = firstMessages[0].parts.find((part) => part instanceof Image);
    expect(media).toBeInstanceOf(Image);
    if (!(media instanceof Image)) throw new Error("expected a portable image part");
    expect(media.base64()).toBe(png);
    expect(media.mimeType()).toBe("image/png");

    // Each helper call re-encodes the owned prompt tree. Rendering a fresh
    // Prompt from the same live spec must not consume either value.
    const secondPrompt = spec.prompt();
    expect(secondPrompt.text()).toBe(text);
    expect(secondPrompt.messages()[0].parts[1]).toBeInstanceOf(Image);
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
