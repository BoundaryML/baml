// === Enum, class, and union throws coverage ===

// --- Enum definitions ---

enum ErrorKind {
  NotFound
  Unauthorized
  ValidationFailed
  RateLimited
}

// --- Class definitions ---

class ApiError {
  code: int
  message: string
}

class ValidationError {
  field: string
  reason: string
}

// === Throwing enums ===

// Throw a specific enum variant
function throw_enum_variant(x: int) -> string throws ErrorKind {
  if (x == 0) { throw ErrorKind.NotFound }
  if (x == 1) { throw ErrorKind.Unauthorized }
  "ok"
}

// Lambda that throws an enum variant
function test_lambda_throws_enum() -> int {
  let f = () -> int { throw ErrorKind.ValidationFailed }
  f()
}

// === Throwing classes ===

// Throw a class instance
function throw_class_instance(x: int) -> string throws ApiError {
  if (x < 0) {
    throw ApiError { code: 500, message: "internal error" }
  }
  "ok"
}

// Lambda that throws a class
function test_lambda_throws_class() -> int {
  let f = () -> int {
    throw ValidationError { field: "email", reason: "invalid format" }
  }
  f()
}

// === Union throws types ===

// Union of enum variants (single enum type covers all)
function throw_various_errors(x: int) -> string throws ErrorKind {
  match (x) {
    0 => throw ErrorKind.NotFound,
    1 => throw ErrorKind.Unauthorized,
    2 => throw ErrorKind.ValidationFailed,
    _ => "ok"
  }
}

// Union of different classes
function throw_mixed_classes(x: int) -> string throws ApiError | ValidationError {
  if (x == 0) {
    throw ApiError { code: 404, message: "not found" }
  }
  if (x == 1) {
    throw ValidationError { field: "id", reason: "required" }
  }
  "ok"
}

// Union of enum and class
function throw_enum_or_class(x: int) -> string throws ErrorKind | ApiError {
  if (x == 0) { throw ErrorKind.RateLimited }
  if (x == 1) { throw ApiError { code: 503, message: "unavailable" } }
  "ok"
}

// Union of primitive, enum, and class
function throw_any_error(x: int) -> string throws string | ErrorKind | ApiError {
  match (x) {
    0 => throw "simple string error",
    1 => throw ErrorKind.NotFound,
    2 => throw ApiError { code: 400, message: "bad request" },
    _ => "ok"
  }
}

// === HOF with enum/class throws ===

// HOF with callback that throws enum
function apply_may_throw_enum(f: () -> int throws ErrorKind) -> int {
  f()
}

function test_apply_enum_thrower() -> int {
  apply_may_throw_enum(() -> int { throw ErrorKind.Unauthorized })
}

// HOF with callback that throws class
function apply_may_throw_class(f: () -> int throws ApiError) -> int {
  f()
}

function test_apply_class_thrower() -> int {
  apply_may_throw_class(() -> int {
    throw ApiError { code: 401, message: "unauthorized" }
  })
}

// HOF with implicit rethrows - enum callback
function apply_generic_enum(f: () -> int) -> int { f() }

function test_rethrows_enum() -> int {
  apply_generic_enum(() -> int { throw ErrorKind.NotFound })
}

// HOF with implicit rethrows - class callback
function apply_generic_class(f: () -> int) -> int { f() }

function test_rethrows_class() -> int {
  apply_generic_class(() -> int {
    throw ValidationError { field: "name", reason: "too short" }
  })
}

// === Catch patterns with enums/classes ===

function catch_enum_variants(x: int) -> string {
  throw_enum_variant(x) catch (e) {
    ErrorKind.NotFound => "not found",
    ErrorKind.Unauthorized => "unauthorized",
    _ => "other error"
  }
}

function catch_class_error(x: int) -> string {
  throw_class_instance(x) catch (e) {
    _: ApiError => "api error: " + e.message
  }
}

function catch_mixed_errors(x: int) -> string {
  throw_enum_or_class(x) catch (e) {
    ErrorKind.RateLimited => "rate limited",
    _: ApiError => "api error",
    _ => "unknown"
  }
}

// === Stored callbacks with enum/class throws ===

class EnumThrowingHandler {
  run: () -> null throws ErrorKind
}

class ClassThrowingHandler {
  run: () -> null throws ApiError
}

class MixedThrowingHandler {
  run: () -> null throws ErrorKind | ApiError
}

function make_enum_handler() -> EnumThrowingHandler {
  EnumThrowingHandler { run: () -> null { throw ErrorKind.NotFound } }
}

function make_class_handler() -> ClassThrowingHandler {
  ClassThrowingHandler {
    run: () -> null { throw ApiError { code: 500, message: "fail" } }
  }
}

// === Type aliases with enum/class throws ===

type EnumThrower = () -> int throws ErrorKind
type ClassThrower = () -> int throws ApiError
type MixedThrower = () -> int throws ErrorKind | ApiError | string

function use_enum_thrower(f: EnumThrower) -> int { f() }
function use_class_thrower(f: ClassThrower) -> int { f() }
function use_mixed_thrower(f: MixedThrower) -> int { f() }

function test_type_alias_enum() -> int {
  use_enum_thrower(() -> int { throw ErrorKind.ValidationFailed })
}

function test_type_alias_class() -> int {
  use_class_thrower(() -> int { throw ApiError { code: 422, message: "invalid" } })
}

function test_type_alias_mixed() -> int {
  use_mixed_thrower(() -> int { throw "string error" })
}
