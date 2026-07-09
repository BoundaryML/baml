//! Phase D regression: a USER-package `//baml:llm_companion` driver whose EXTRA
//! parameter is typed as a USER CLASS must generate working `Foo$<suffix>`
//! companions for LLM functions in EVERY namespace.
//!
//! Before the fix, `make_user_drive_companion` (baml_compiler2_ppir) spliced the
//! driver's extra-param `TypeExpr`s verbatim into the generated companion. Those
//! TypeExprs carry file-relative paths, so a bare user-class name (`Cfg`, defined
//! in the driver's namespace) landed unqualified in a FOREIGN namespace's
//! companion and either failed to resolve (a corpus-wide compile error) or, worse,
//! mis-resolved to the wrong type and hung at runtime. Same family as the known
//! "driver call paths must be `root.`-absolute" gotcha — the DRIVER PATH was
//! already root-absolutized; the extras' TYPES were missed.

use baml_tests::engine::{IndexMap, compile_multi_file, run_compiled};
use bex_engine::BexExternalValue;

/// File A lives in namespace `alpha`: it declares the `Cfg` user class, the
/// `//baml:llm_capability` interface, the `//baml:llm_companion(guarded)` driver
/// (extra param `cfg: Cfg`), and an offline provider that reads `cfg.level`.
const ALPHA: &str = r#"
class Cfg { level: int }

//baml:llm_capability
interface Guarded requires baml.ai.Provider {
    function call_guarded<T>(self, messages: baml.ai.ChatMessage[], cfg: Cfg) -> T
        throws baml.errors.UnknownError
}

//baml:llm_companion(guarded)
function drive_guarded<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst, cfg: Cfg) -> T
    throws baml.errors.Unsupported | baml.errors.UnknownError {
    let p = client;
    match (p) {
        let m: Guarded => m.call_guarded<T>(baml.ai.prompt_to_messages(prompt), cfg),
        _ => throw baml.errors.Unsupported { message: "client's provider is not Guarded" },
    }
}

class GuardedEcho {
    reply: string,

    implements baml.ai.Provider {}

    implements Guarded {
        function call_guarded<T>(self, messages: baml.ai.ChatMessage[], cfg: Cfg) -> T
            throws baml.errors.UnknownError {
            baml.sap.parse<T>("[lvl" + cfg.level.to_string() + "] " + self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["guarded parse failed"] },
            }
        }
    }
}
"#;

/// File B lives in the ROOT namespace: a DIFFERENT namespace from the driver, so
/// its generated `ComposeNote$guarded` companion is the crash site. It references
/// the alpha-namespace symbols through the `root.`-absolute form.
const ROOT_MAIN: &str = r##"
client<llm> DeadGuarded {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ComposeNote(q: string) -> string {
  client DeadGuarded
  prompt #"Compose a note about: {{ q }}"#
}

function main() -> string {
    ComposeNote$guarded(
        "turtles",
        root.alpha.Cfg { level: 3 },
        client = root.alpha.GuardedEcho { reply: "a draft note" },
    ) catch (e) {
        _ => "companion failed",
    }
}
"##;

/// The generated companion for an LLM function in a foreign namespace compiles
/// (its `cfg: Cfg` extra resolves) and routes through the user provider at
/// runtime, threading the user-class extra all the way through.
#[tokio::test]
async fn user_class_extra_param_routes_cross_namespace() {
    let program = compile_multi_file(&[("ns_alpha/cap.baml", ALPHA), ("main.baml", ROOT_MAIN)]);
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("[lvl3] a draft note".into())
    );
}
