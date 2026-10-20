// Syntax that lowers to calls into the standard library — `spawn { .. }`,
// `env.X`, a `prompt` template, an LLM function's whole body — names the
// library's packages by IDENTITY. A binding in scope that happens to be called
// `baml`, `ai` or `openai`, or a namespace of this package called `ai` (see
// `ns_ai/` and `ns_baml/` beside this file), is ordinary: it does not capture
// the lowered code, and the lowered code does not stop it from being used.
//
// Compiler side: every path a desugar in crates/baml_compiler2_ast writes to
// name a standard-library declaration is marked compiler-written
// (`PathOrigin::Compiler`), and HIR reads its first segment as the language
// package by identity (`PathHead::Lang`), never as a name of the scope it
// lands in. A desugar that names an item of the writer's own namespace — a
// static method's companion call `Owner.Method@spec` — is marked the same way
// and read as that item, in no scope. (The prompt-local `role` and `cache`
// are still read in scope; see the `// BUG:` note in `lower_expr_body.rs`.)
//
// Corpus convention: every case body is a top-level `function`, `test` blocks
// only assert.

// ─── an LLM function's parameters ────────────────────────────────────────────

// The parameters are in scope of everything the declaration lowers to: the
// client constructor (`openai.…`), the agent and its toolbox (`ai.…`), the
// output parser (`baml.…`).
function HygieneLiteralClient(ai: string, openai: string, baml: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize: ${ai} ${openai} ${baml}`
}

function hygiene_selector() -> string {
    "anthropic/claude-sonnet-4-5"
}

// A dynamic selector lowers to `ai.clients.resolve` with a table that names
// every provider package.
function HygieneDynamicClient(ai: string, anthropic: string, google: string) -> string {
    client: hygiene_selector()
    prompt: `Summarize: ${ai} ${anthropic} ${google}`
}

// Building the bound spec runs the lowering — the client is constructed, the
// arguments recorded — without calling a model.
function hygiene_literal_spec_name() -> string {
    HygieneLiteralClient@spec("a", "b", "c").spec_name
}

function hygiene_dynamic_spec_name() -> string {
    HygieneDynamicClient@spec("a", "b", "c").spec_name
}

// ─── a desugar into the writer's own namespace ───────────────────────────────

// A static LLM method's body calls its companion through its class
// (`HygieneStat.Describe@spec`). The class is named as an item of this
// namespace, so a parameter spelled like the class does not capture it.
class HygieneStat {
    function Describe(HygieneStat: string) -> string {
        client: "openai/gpt-4o-mini"
        prompt: `Describe: ${HygieneStat}`
    }
}

function hygiene_static_method_parameter_named_like_its_class() -> string {
    HygieneStat.Describe@spec("x").spec_name
}

// ─── every way a body binds a name ───────────────────────────────────────────

function hygiene_local() -> int {
    let baml = 5;
    let task = spawn { baml + 1 };
    await task
}

function hygiene_parameter(baml: int) -> int {
    let task = spawn { baml + 1 };
    await task
}

function hygiene_lambda_parameter() -> int {
    let run = (baml: int) -> int {
        let task = spawn { baml + 1 };
        await task
    };
    run(5)
}

class HygieneBox { baml: int }

function hygiene_destructured(box: HygieneBox) -> int {
    let HygieneBox { baml } = box;
    let task = spawn { baml + 1 };
    await task
}

function hygiene_for_in(xs: int[]) -> int {
    let total = 0;
    for (let baml in xs) {
        let task = spawn { baml + 1 };
        total += await task;
    }
    total
}

function hygiene_static_type_binding() -> int {
    let type baml = int.type();
    let task = spawn { 6 };
    await task
}

function hygiene_runtime_type_binding(t: reflect.Type) -> int {
    let type baml = t;
    let task = spawn { 6 };
    await task
}

// `env.X` under a binding named `baml`: still the late-bound reference.
function hygiene_env(baml: string) -> string {
    env.BAML_TEST_HYGIENE_VAR.name + baml
}

// A bare `prompt` template tag under a binding named `ai`.
function hygiene_prompt_tag() -> int {
    let ai = 1;
    let rendered = prompt`hello ${ai}`;
    ai
}

// ─── items, generics and catch bindings named like packages ──────────────────

// An ITEM of this namespace named `baml` is as ordinary as a binding: the
// lowered `spawn` names the language package by identity. (A WRITTEN
// `baml.x` in this namespace would read the class — an own item shadows a
// package entirely, see `ns_reflect_shadowing`.)
class baml { label: string }

function hygiene_item_named_baml() -> int {
    let b = baml { label: "xyz" };
    let task = spawn { b.label.length() };
    await task
}

// Generic parameters named like the packages a body's lowering uses.
function hygiene_generics<ai, baml>(x: ai, y: baml) -> int {
    let task = spawn { 2 };
    await task
}

// A catch binding named `baml`.
function hygiene_catch_binding() -> int {
    let caught = { throw "x" } catch (baml) { _ => 3 };
    let task = spawn { caught };
    await task
}

// `#4543`: `reflect` is a package a local shadows like any other.
function hygiene_reflect_local() -> int {
    let reflect = 1;
    reflect + 1
}

// `${x}` lowers to `baml.String.from(x)`, the primitive's companion by
// identity: a parameter named `string` does not capture it.
function hygiene_string_binding(string: int) -> string {
    `value: ${string}`
}

// `$init_test` binds its collector under a name no body can spell, so a
// user function named `registry` is callable from a test (see the test
// block below).
function registry() -> int {
    4
}

// ─── this package's own namespaces ───────────────────────────────────────────

// This package's own namespaces named `ai` and `baml` (`ns_ai/shadow.baml`,
// `ns_baml/shadow.baml`, nested under this one) are reached as every
// namespace is, by their full `root.` path; a written `ai.Client` is the
// standard library's, and so is the lowered `ai.Client` of the LLM functions
// above. Neither reading disturbs the other.
function hygiene_own_namespace() -> int {
    root.desugar_hygiene.ai.Client { marker: 7 }.marker
        + root.desugar_hygiene.baml.TaggedString { marker: 1 }.marker
}

// A ROOT-level namespace of this package named `ai` (`ns_ai/` beside the
// corpus root) is reached as `root.ai.…` and captures nothing either.
function hygiene_root_namespace() -> int {
    root.ai.Client { marker: 9 }.marker
}

test "an LLM function's parameters may be named like the packages its lowering uses" {
    assert.equal(hygiene_literal_spec_name(), "HygieneLiteralClient");
    assert.equal(hygiene_dynamic_spec_name(), "HygieneDynamicClient")
}

test "a static LLM method's parameter may be named like its class" {
    assert.equal(hygiene_static_method_parameter_named_like_its_class(), "Describe")
}

test "a binding named like a package does not capture lowered syntax" {
    assert.equal(hygiene_local(), 6);
    assert.equal(hygiene_parameter(5), 6);
    assert.equal(hygiene_lambda_parameter(), 6);
    assert.equal(hygiene_destructured(HygieneBox { baml: 5 }), 6);
    assert.equal(hygiene_for_in([1, 2]), 5);
    assert.equal(hygiene_static_type_binding(), 6);
    assert.equal(hygiene_runtime_type_binding(string.type()), 6);
    assert.equal(hygiene_env("!"), "BAML_TEST_HYGIENE_VAR!");
    assert.equal(hygiene_prompt_tag(), 1)
}

test "items, generics and catch bindings named like a package do not capture lowered syntax" {
    assert.equal(hygiene_item_named_baml(), 3);
    assert.equal(hygiene_generics<int, int>(1, 2), 2);
    assert.equal(hygiene_catch_binding(), 3);
    assert.equal(hygiene_reflect_local(), 2);
    assert.equal(hygiene_string_binding(7), "value: 7");
    assert.equal(registry(), 4)
}

test "a namespace named like a package does not capture lowered syntax" {
    assert.equal(hygiene_own_namespace(), 8);
    assert.equal(hygiene_root_namespace(), 9)
}
