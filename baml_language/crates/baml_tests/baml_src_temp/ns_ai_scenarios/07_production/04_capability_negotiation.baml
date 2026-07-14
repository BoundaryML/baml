test "guide 07.04: erased providers can be negotiated explicitly" {
  let providers: root.ai.Provider[] = [
    fake_resolution(),
    root.ai.FakeRealtimeProvider { transcript: "hello" },
  ];
  let capabilities = providers.map((provider: root.ai.Provider) -> string {
    match (provider) {
      let realtime: root.ai.RealtimeProvider => "realtime",
      let drive: root.ai.DriveProvider => "drive",
      _ => "provider-only",
    }
  });
  assert.equal(capabilities, ["drive", "realtime"])
}

