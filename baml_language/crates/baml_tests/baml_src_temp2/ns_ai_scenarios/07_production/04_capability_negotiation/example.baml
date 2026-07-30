// Accept provider configuration without knowing its concrete type and select
// the strongest compatible execution mode automatically.
function capability_negotiation() -> NegotiatedRun[] {
    let providers: ai.Provider[] = [fake_resolution(), ai.testing.FakeRealtimeProvider { transcript: "negotiated realtime" }];
    let runs = providers.map((provider: ai.Provider) -> NegotiatedRun {
        ResolveTicket@task(sample_ticket()).with_provider(provider)
            .run(runner = CapabilityNegotiationRunner.new())
    });
    log.info(runs);
    runs
}
