// Live public-API coverage for Google-native function calling. The Agent runner
// executes the application tool and continues until the provider returns T.

testset "integ-test-google-ai-native-tools" {
    test "Google AI calls an application tool before returning the typed result" {
        let provider = root
            .google
            .ai
            .gemini(
                api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
                output_mode = root.google.ai.OutputMode.Native,
                tool_mode = root.google.ai.ToolMode.Native,
            );
        let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
        let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
        match (outcome) {
            let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
            _ => assert.is_true(false),
        }
    }
}

testset "integ-test-google-vertex-native-tools" {
    test "Google Vertex calls an application tool before returning the typed result" {
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
                    tool_mode = root.google.vertex.ToolMode.Native,
                );
            let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
            let outcome = task.run(runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
            match (outcome) {
                let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
                _ => assert.is_true(false),
            }
        }
    }
}
