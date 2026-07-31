//! Coherence gate (BEPv4 D11): a trivial user project must compile cleanly
//! against the embedded stdlib. The stdlib is type-checked together with
//! every user project, so a single unresolved name inside `baml_std`
//! (e.g. an unqualified cross-namespace reference) breaks every downstream
//! project at once — this test makes that a build failure here instead of a
//! support ticket later.

use baml_tests::engine::compile_source;

#[tokio::test]
async fn empty_project_compiles_against_embedded_stdlib() {
    // compile_source asserts there are no diagnostic errors, which includes
    // diagnostics raised inside the embedded stdlib itself.
    let _ = compile_source("function main() -> int { 42 }");
}

#[tokio::test]
async fn ai_surface_compiles_against_embedded_stdlib() {
    // Touch the ai package's public surface so stdlib regressions in the
    // AI namespaces (not just baml core) are caught by the gate too.
    let _ = compile_source(
        r#"
        class Answer {
            reply: string,
        }

        function Ask(topic: string) -> Answer {
            provider: ai.testing.fake_output_provider(`{"reply":"ok"}`)
            prompt: `
                Answer about ${topic}.
                ${ctx.output_format}
            `
        }

        function main() -> string throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
            let session = ai.run.AgentSession<Answer>.start(Ask@task("checks"));
            match (session.send("hello")) {
                let done: ai.Done<Answer> => done.value.reply,
                _ => "incomplete",
            }
        }
        "#,
    );
}
