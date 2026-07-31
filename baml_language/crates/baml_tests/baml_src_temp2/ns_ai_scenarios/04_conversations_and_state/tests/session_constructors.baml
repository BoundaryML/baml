// D2/D3 surface: start (session-first entry), from (one constructor for
// both continuation poles), string-first send, and move_to (one-call
// provider switch).

function _constructor_provider(name: string) -> ai.testing.FakeToolProvider throws never {
    ai.testing.FakeToolProvider {
        provider_name: name,
        tool_calls: [],
        provider_tools: [],
        final_output: `
            {
                  "category":"billing",
                  "priority":"Urgent",
                  "summary":"Resolved",
                  "reply":"The duplicate is a pending authorization."
                }
        `,
    }
}

test "D2: start gives a session from the get-go; string send runs turn one" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_constructor_provider("fake-a"));
    let session = ai.run.AgentSession<Resolution>.start(task);
    //# Turn one and turn N are the same code path, and a plain string is
    //# the everyday call.
    let outcome = session.send("Also check invoice 42.");
    match (outcome) {
        let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
        _ => assert.is_true(false),
    }
}

test "D2: resume on a started session runs the rendered prompt without a message" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_constructor_provider("fake-a"));
    let session = ai.run.AgentSession<Resolution>.start(task);
    let outcome = session.resume();
    match (outcome) {
        let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
        _ => assert.is_true(false),
    }
}

test "D2: from accepts an exact conversation" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_constructor_provider("fake-a"));
    let first = task.run(runner = ai.run.Agent<Resolution>.new());
    let conversation = match (first) {
        let done: ai.Done<Resolution> => done.conversation,
        _ => baml.sys.panic("expected Done"),
    };
    let session = ai.run.AgentSession<Resolution>.from(task, conversation);
    match (session.send("One more question.")) {
        let done: ai.Done<Resolution> => assert.equal(done.value.priority, TicketPriority.Urgent),
        _ => assert.is_true(false),
    }
}

test "D2: from accepts portable messages and reconstructs through the provider" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_constructor_provider("fake-a"));
    let history = ai.MessageHistory {
        entries: [
            ai.ChatMessage.user("I was charged twice."),
            ai.ChatMessage.assistant("Let me check."),
        ],
    };
    let session = ai.run.AgentSession<Resolution>.from(task, history);
    assert.equal(session.conversation().messages().items().length(), 2);
    match (session.send("Continue.")) {
        let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
        _ => assert.is_true(false),
    }
}

test "D3: move_to switches provider in one call and is non-destructive" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_constructor_provider("fake-a"));
    let session = ai.run.AgentSession<Resolution>.start(task);
    let first = session.send("Start on provider A.");
    match (first) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }

    let moved = session.move_to(_constructor_provider("fake-b"));
    assert.equal(moved.conversation().provider().name(), "fake-b");
    //# The original session is unchanged and still valid on provider A.
    assert.equal(session.conversation().provider().name(), "fake-a");

    match (moved.send("Continue on provider B.")) {
        let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
        _ => assert.is_true(false),
    }
}

test "D3: move_to refuses a session with unanswered tool calls" {
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(ai.testing.FakeToolProvider {
        provider_name: "fake-a",
        tool_calls: [
            ai.tools.ToolCall { id: "h-1", name: "lookup_account", args: { "customer_id": "C-1" } },
        ],
        provider_tools: [],
        final_output: `{"category":"billing","priority":"Urgent","summary":"s","reply":"r"}`,
    }).with_tools([handoff_tool]);
    let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
    let session = ai.run.AgentSession<Resolution>.of(task, outcome);
    let kind = session.move_to(_constructor_provider("fake-b")).conversation().provider().name() catch (e) {
        let invalid: ai.InvalidRequest => {
            assert.is_true(invalid.detail.includes("unanswered tool calls"));
            "refused"
        },
        _ => "wrong-channel",
    };
    assert.equal(kind, "refused")
}
