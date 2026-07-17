function NewCancelToken() -> baml.spawn.CancelToken {
  baml.spawn.CancelToken.new()
}

function AnyCancelToken(tokens: baml.spawn.CancelToken[]) -> baml.spawn.CancelToken {
  baml.spawn.CancelToken.any(tokens)
}

function RoundTripCancelToken(token: baml.spawn.CancelToken) -> baml.spawn.CancelToken {
  token
}

function CancelSpawnAndPropagate() -> int {
  let token = baml.spawn.CancelToken.new();
  let future = spawn with baml.spawn.options(cancel = token) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    42
  };
  let _ = token.cancel();
  await future
}
