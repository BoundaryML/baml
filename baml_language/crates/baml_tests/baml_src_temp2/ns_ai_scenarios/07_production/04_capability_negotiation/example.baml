// Accept provider configuration without knowing its concrete type and select
// the strongest compatible execution mode automatically.
function capability_negotiation() -> NegotiatedRun[] {
    let providers: root.ai.Provider[] = [fake_resolution(), root.ai.FakeRealtimeProvider { transcript: "negotiated realtime" }];
    let runs = providers.map((provider: root.ai.Provider) -> NegotiatedRun {
        ResolveTicket_task(sample_ticket(), provider)
            .run(runner = CapabilityNegotiationRunner.new())
    });
    log.info(runs);
    runs
}
