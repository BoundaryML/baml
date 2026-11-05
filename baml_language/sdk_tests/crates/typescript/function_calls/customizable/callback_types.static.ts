import { callback_probe, callback_probe_sync } from "./baml_sdk/index.js";
import {
  call_callback_with_optional_args_all_set,
  call_int_callback,
  call_returned_callback,
  call_returned_callback_in_list,
  call_with_typed_throws,
  hof,
  make_adder_sync,
  ValidationError,
} from "./baml_sdk/host_callable_tests/index.js";
import { Greeter } from "./baml_sdk/methods_on_classes/index.js";

// Compiled by every Node/Web/Workers fixture tsc check; never executed.
// The plain function name is the async entry point (formerly `_async`).
const asyncResult: Promise<string> = callback_probe(async value => value);
const syncCallbackResult: Promise<string> = callback_probe(value => value);
// Node rejects host callbacks at runtime on the sync path; its result type stays synchronous.
const syncEntryResult: string = callback_probe_sync(value => value);

// @ts-expect-error a synchronous entry point still returns a value
const wrongSyncResult: Promise<string> = callback_probe_sync(value => value);
// @ts-expect-error an async entry point still returns a Promise
const wrongAsyncResult: string = callback_probe(value => value);
// @ts-expect-error callbacks must return the declared value type
callback_probe(value => value.length);
// @ts-expect-error Promise results must resolve to the declared value type
callback_probe(async value => value.length);
// @ts-expect-error a Promise must not hide an invalid union arm
callback_probe(async value => value.length ? value : 0);
// @ts-expect-error synchronous callback returns are checked too
callback_probe_sync(value => value.length);

// An inferred or explicit throws contract never widens the successful result.
call_int_callback(async value => value + 1, 1);
call_with_typed_throws(async value => String(value), 1);
// @ts-expect-error a declared thrown value is not a successful callback result
call_with_typed_throws(async () => new ValidationError({ code: 422, message: "invalid", fields: [] }), 1);

// Optional callback arguments and instance methods use the same translation.
call_callback_with_optional_args_all_set(async (value, options) => value + (options?.y ?? 0), 1);
const greeter = new Greeter({ name: "Ada" });
greeter.wait(async () => "ready");
// @ts-expect-error instance method callbacks also check their resolved type
greeter.wait(async () => 1);

// Nested callbacks returned by host factories can be async at either level.
call_returned_callback(() => async value => String(value), 1);
call_returned_callback(async () => value => String(value), 1);
call_returned_callback(async () => async value => String(value), 1);
call_returned_callback_in_list(async () => [async value => String(value)], 1);
// @ts-expect-error a nested async callback must resolve to its declared result
call_returned_callback(async () => async value => value, 1);
// @ts-expect-error container nesting must not erase callback result types
call_returned_callback_in_list(async () => [async value => value], 1);

// A closure made by BAML remains async with a synchronous callSync sibling.
hof(async callback => {
  const asyncValue: Promise<number> = callback(1);
  const syncValue: number = callback.callSync(1);
  // @ts-expect-error the synchronous sibling must not return a Promise
  const wrong: Promise<number> = callback.callSync(1);
  return await asyncValue + syncValue;
});
call_int_callback(make_adder_sync(1), 1);
