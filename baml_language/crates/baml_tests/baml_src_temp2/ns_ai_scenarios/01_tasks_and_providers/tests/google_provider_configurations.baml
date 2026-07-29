// Live public-API coverage for typed Google generation. Every case runs a
// Task through the ordinary completion runner; provider internals are not part
// of this contract.

function _google_resolution_array_task(
    provider: root.ai.Provider,
) -> root.ai.Task<Resolution[]> throws never {
    root.ai.task_named<Resolution[]>(provider, "GoogleResolutionArray", {}, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    "Return exactly two support resolutions for a duplicate charge. Give each a non-empty reply.",
                    ctx.output_format,
                ],
            )
    })
}

test "Google AI public configuration owns its request-facing options" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "fixture-model",
            api_key = "fixture-key",
            base_url = "https://example.invalid/v1beta",
            output_mode = root.google.ai.OutputMode.Sap,
            tool_mode = root.google.ai.ToolMode.Prompt,
        );
    let task = ResolveTicket_task(sample_ticket(), provider);
    assert.equal(provider.model, "fixture-model");
    assert.equal(provider.api_key, "fixture-key");
    assert.equal(provider.base_url, "https://example.invalid/v1beta");
    assert.equal(provider.output_mode, root.google.ai.OutputMode.Sap);
    assert.equal(provider.tool_mode, root.google.ai.ToolMode.Prompt);
    assert.equal(provider.render_shorthand(), "google-ai/fixture-model");
    assert.equal(task.provider_name(), "google-ai")
}

test "Google Vertex public configuration owns its backend selection" {
    let provider = root
        .google
        .vertex
        .gemini(
            model = "fixture-model",
            project_id = "fixture-project",
            location = "us-central1",
            output_mode = root.google.vertex.OutputMode.Native,
            tool_mode = root.google.vertex.ToolMode.Native,
        );
    let task = ResolveTicket_task(sample_ticket(), provider);
    assert.equal(provider.model, "fixture-model");
    assert.equal(provider.project_id, "fixture-project");
    assert.equal(provider.location, "us-central1");
    assert.equal(provider.output_mode, root.google.vertex.OutputMode.Native);
    assert.equal(provider.tool_mode, root.google.vertex.ToolMode.Native);
    assert.equal(provider.render_shorthand(), "vertex-ai/fixture-model");
    assert.equal(task.provider_name(), "google-vertex")
}

testset "integ-test-google-vertex-configurations" {
    test "Google Vertex native completes a class-valued task" {
        let enabled = baml.env.get("RUN_VERTEX_INTEGRATION_TESTS") == "true";
        let project_id = baml.env.get("GOOGLE_CLOUD_PROJECT");
        let location = baml.env.get("GOOGLE_CLOUD_LOCATION");
        if (!enabled || project_id == null || location == null) {
            log.info("skipping opt-in Google Vertex live test");
            assert.is_true(true)
        } else {
            let provider = root
                .google
                .vertex
                .gemini(
                    project_id = project_id,
                    location = location,
                    output_mode = root.google.vertex.OutputMode.Native,
                );
            let value = ResolveTicket_task(sample_ticket(), provider)
                .run(runner = root.ai.run.Completion<Resolution>.new());
            assert.is_true(value.reply.length() > 0)
        }
    }

    test "Google Vertex SAP completes a class-valued task" {
        let enabled = baml.env.get("RUN_VERTEX_INTEGRATION_TESTS") == "true";
        let project_id = baml.env.get("GOOGLE_CLOUD_PROJECT");
        let location = baml.env.get("GOOGLE_CLOUD_LOCATION");
        if (!enabled || project_id == null || location == null) {
            log.info("skipping opt-in Google Vertex live test");
            assert.is_true(true)
        } else {
            let provider = root
                .google
                .vertex
                .gemini(
                    project_id = project_id,
                    location = location,
                    output_mode = root.google.vertex.OutputMode.Sap,
                );
            let value = ResolveTicket_task(sample_ticket(), provider)
                .run(runner = root.ai.run.Completion<Resolution>.new());
            assert.is_true(value.summary.length() > 0)
        }
    }
}

testset "integ-test-google-ai-configurations" {
    test "Google AI native completes a class-valued task" {
        let provider = root
            .google
            .ai
            .gemini(
                api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
                output_mode = root.google.ai.OutputMode.Native,
            );
        let value = ResolveTicket_task(sample_ticket(), provider)
            .run(runner = root.ai.run.Completion<Resolution>.new());
        assert.is_true(value.reply.length() > 0)
    }

    test "Google AI native keeps an array inside the typed result" {
        let provider = root
            .google
            .ai
            .gemini(
                api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
                output_mode = root.google.ai.OutputMode.Native,
            );
        let value = _google_resolution_array_task(provider)
            .run(runner = root.ai.run.Completion<Resolution[]>.new());
        assert.equal(value.length(), 2);
        assert.is_true(value[0].reply.length() > 0);
        assert.is_true(value[1].reply.length() > 0)
    }

    test "Google AI SAP completes a class-valued task" {
        let provider = root
            .google
            .ai
            .gemini(
                api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
                output_mode = root.google.ai.OutputMode.Sap,
            );
        let value = ResolveTicket_task(sample_ticket(), provider)
            .run(runner = root.ai.run.Completion<Resolution>.new());
        assert.is_true(value.summary.length() > 0)
    }

    test "Google AI preserves a structural image through completion" {
        let provider = root
            .google
            .ai
            .gemini(
                api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
                output_mode = root.google.ai.OutputMode.Sap,
            );
        let task = InspectImage_task(
            image
                .from_url(
                    "https://storage.googleapis.com/cloud-samples-data/generative-ai/image/scones.jpg",
                    "image/jpeg",
                ),
            provider,
        );
        let value = task.run(runner = root.ai.run.Completion<string>.new());
        assert.is_true(value.length() > 0)
    }
}
