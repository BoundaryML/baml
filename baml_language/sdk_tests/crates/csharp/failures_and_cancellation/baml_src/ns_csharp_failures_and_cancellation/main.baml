class Failure {
  code int
  detail string
}

function ping() -> int {
  42
}

function throw_failure() -> int throws Failure {
  throw Failure { code: 42, detail: "sensitive failure detail" }
}

function throw_type_mismatch() -> int throws baml.errors.TypeMismatch {
  throw baml.errors.TypeMismatch { message: "sensitive mismatch detail" }
}

function do_panic(message: string) -> int {
  baml.sys.panic(message);
  0
}

function do_exit(code: int) -> int {
  baml.sys.exit(code);
  0
}

function sleep_ms(ms: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
  1
}

function engine_cancel() -> int {
  let tok = baml.spawn.CancelToken.new();
  let f = spawn with baml.spawn.options(cancel = tok) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
    42
  };
  let _ = tok.cancel();
  await f
}
