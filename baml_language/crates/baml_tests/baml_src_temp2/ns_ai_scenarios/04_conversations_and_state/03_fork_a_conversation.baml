// Explore two independent follow-up paths from the same conversation without
// either branch changing the other.
//
// This is the PORTABLE pole: an application-owned MessageHistory forked by
// value, surviving provider switches but giving up exact continuation state.
// The exact-state counterpart is `session.fork()` — see
// 06_agent_session.baml — which keeps thinking signatures and server-side
// state but stays with the conversation's provider.
function fork_a_conversation() -> ConversationProbe[] {
    let base = sample_history();
    let billing = base.append(ai.ChatMessage.user("Please refund it.")).to_history();
    let fraud = base.append(ai.ChatMessage.user("Could this be fraud?")).to_history();

    let billing_result = done_or_fail(
            ProbeConversation@task(billing).with_provider(ai
                    .testing
                    .fake_output_provider(
                        `
                            {
                                    "branch": "billing",
                                    "evidence": "Please refund it."
                                  }
                        `,
                    ))
            .run(runner = ai.run.Agent<ConversationProbe>.new()),
        )
        .value;
    let fraud_result = done_or_fail(
            ProbeConversation@task(fraud).with_provider(ai
                    .testing
                    .fake_output_provider(
                        `
                            {
                                    "branch": "fraud",
                                    "evidence": "Could this be fraud?"
                                  }
                        `,
                    ))
            .run(runner = ai.run.Agent<ConversationProbe>.new()),
        )
        .value;

    let results = [billing_result, fraud_result];
    log.info(
        {
            "base_messages": base.entries.length(),
            "billing_messages": billing.entries.length(),
            "fraud_messages": fraud.entries.length(),
            "results": results,
        },
    );
    results
}
