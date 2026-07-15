import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import { OptBox, optional_args_probe, optional_args_probe_async } from "./baml_sdk/index.js";

describe("function_calls — optional_args_probe", () => {
  it("test_optional_args_runtime_matrix", () => {
    expect(optional_args_probe(1)).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, {})).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, undefined)).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, { opt1: null })).toEqual([1, null, 99]);
    expect(optional_args_probe(1, { opt1: 8 })).toEqual([1, 8, 99]);
    expect(optional_args_probe(1, { opt2: null })).toEqual([1, 5, null]);
    expect(optional_args_probe(1, { opt2: 9 })).toEqual([1, 5, 9]);
    expect(optional_args_probe(1, { opt1: null, opt2: null })).toEqual([1, null, null]);
    expect(optional_args_probe(1, { opt1: 8, opt2: 9 })).toEqual([1, 8, 9]);
  });

  it("test_python_unset_and_none_differ_in_one_call", () => {
    expect(optional_args_probe(1, { opt1: undefined })).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, { opt1: undefined, opt2: null })).toEqual([1, 5, null]);
  });

  it("test_optional_args_async_samples", async () => {
    expect(await optional_args_probe_async(1)).toEqual([1, 5, 99]);
    expect(await optional_args_probe_async(1, { opt1: null })).toEqual([1, null, 99]);
    expect(await optional_args_probe_async(1, { opt2: 9 })).toEqual([1, 5, 9]);
  });

  it("test_negative_runtime_cases_reject", async () => {
    expect(() => (optional_args_probe as any)(1, { opt3: 1 })).toThrow();
    await expect((optional_args_probe_async as any)(1, { opt3: 1 })).rejects.toThrow();
    expect(() => (optional_args_probe as any)()).toThrow();
  });
});

describe("function_calls — OptBox optional method args", () => {
  it("test_opt_box_method_matrix", () => {
    const box = OptBox.make(10);
    expect(box.base).toBe(17);

    const box2 = OptBox.make(10, { opt1: 0 });
    expect(box2.base).toBe(10);
    expect(box2.probe(1)).toEqual([10, 1, 5]);
    expect(box2.probe(1, { opt1: 8 })).toEqual([10, 1, 8]);
  });
});
