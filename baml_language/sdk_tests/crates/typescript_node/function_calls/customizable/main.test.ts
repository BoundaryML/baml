// Smoke tests for plain (non-LLM) expression functions: the nullary base case,
// a single required argument, and the call forms for a function with required +
// optional (default-valued) parameters.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  hello_world,
  hello_world_async,
  required_with_optional_args,
  single_required_arg,
} from "./baml_sdk/index.js";

describe("function_calls — hello_world", () => {
  it("returns the literal (sync)", () => {
    expect(hello_world()).toBe("hello world");
  });

  it("returns the literal (async)", async () => {
    expect(await hello_world_async()).toBe("hello world");
  });
});

describe("function_calls — single_required_arg", () => {
  it("round-trips a single positional argument", () => {
    // The next step up from the nullary case: one required positional arg.
    expect(single_required_arg("hi")).toBe("hi");
  });
});

// ── required_with_optional_args(arg0: int, opt1: int? = 5, opt2: int? = make_opt2()) ──
//
// `void` return → null on the host, so each call asserts `toBeNull()`; this
// models the *call forms*, not a computed value. `opt1` has a literal default;
// `opt2`'s default is an expression (`make_opt2()`) the engine evaluates when
// `opt2` is omitted.
//
// Two constraints shape what TypeScript can express today:
//   1. JS/TS has no named-argument syntax — every call is positional, so the
//      BAML named forms (`opt1 = 2`, `arg0 = 1`, …) collapse to positional
//      calls. Skipping a *middle* optional (set opt2, leave opt1 defaulted)
//      needs the `UNSET` sentinel from `@boundaryml/baml-core-node`.
//   2. `sdkgen_typescript_node` doesn't thread arg defaults into the surface
//      type, so the generated signature is `(arg0, opt1, opt2)` with all three
//      REQUIRED — any form that omits an optional isn't typeable yet.
//
//   BAML call form                                 │ TS status
//   ───────────────────────────────────────────────┼──────────────────────────────────
//   required_with_optional_args(1, 2, 3)           │ ✓ typed (also models opt1=2, opt2=3)
//   required_with_optional_args(1, null, null)     │ ✓ typed (explicit nulls)
//   required_with_optional_args(1)                 │ TODO — omits opt1 + opt2
//   required_with_optional_args(1, 2)              │ TODO — omits opt2
//   required_with_optional_args(1, opt2 = 3)       │ TODO — skip opt1 via UNSET
//   required_with_optional_args(1, opt1 = null)    │ TODO — omits opt2
//
// TODO(sdkgen_typescript_node): render defaulted params as optional
// (`opt1?: number | null`) and accept `UNSET`, so the omission / skip-middle
// forms below don't need an `as unknown as` cast. Until then the casts exercise
// the runtime contract (the engine fills omitted defaults) that the surface
// type can't yet express.

describe("function_calls — required_with_optional_args, typed forms", () => {
  it("passes all three positionally", () => {
    expect(required_with_optional_args(1, 2, 3)).toBeNull();
  });

  it("passes explicit nulls for both optionals", () => {
    expect(required_with_optional_args(1, null, null)).toBeNull();
  });
});

describe("function_calls — required_with_optional_args, omitted defaults (cast)", () => {
  // TODO(sdkgen_typescript_node): expressible without a cast once defaulted
  // params are emitted as optional. The casts below prove the runtime fills
  // the omitted defaults (opt1 → 5, opt2 → make_opt2()).
  it("omits both optionals", () => {
    const f = required_with_optional_args as unknown as (arg0: number) => null;
    expect(f(1)).toBeNull();
  });

  it("sets opt1 only, omitting opt2", () => {
    const f = required_with_optional_args as unknown as (
      arg0: number,
      opt1: number | null,
    ) => null;
    expect(f(1, 2)).toBeNull();
  });
});
