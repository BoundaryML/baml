import {
  Agent,
  BridgeScope,
  ClientRef,
  DecoderRef,
  FunctionSpec,
  ResponsesClient,
  encodeClient,
  encodeDecoder,
  runtimeSymbolCount,
  types,
  type ClientInput,
  type DecoderHost,
  type DecoderInput,
  type Invoice,
  type Resume,
} from "./contracts.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function expectReject(action: () => unknown | Promise<unknown>, pattern: RegExp): Promise<void> {
  try {
    await action();
  } catch (error) {
    assert(error instanceof Error && pattern.test(error.message), `unexpected rejection: ${String(error)}`);
    return;
  }
  throw new Error(`expected rejection matching ${pattern}`);
}

const session = "runtime-session";
const concrete = new ResponsesClient(session);
const ref = ClientRef.createForProbe(session);
assert(encodeClient(concrete, session) === "checked-client-view", "concrete client projection failed");
assert(encodeClient(ref, session) === "checked-client-view", "client ref projection failed");

// `declare` brands emit no JavaScript property. Runtime trust comes from the
// private WeakMap registry in this probe, standing in for a checked bridge view.
assert(runtimeSymbolCount(concrete) === 0, "compile-time brand unexpectedly emitted at runtime");
assert(runtimeSymbolCount(ref) === 0, "compile-time brand unexpectedly emitted at runtime");

const forgedClient = { id: async () => "forged" } as unknown as ClientInput;
await expectReject(() => encodeClient(forgedClient, session), /no checked Client projection/);
await expectReject(() => encodeClient(concrete, "other-session"), /no checked Client projection/);

const scope = new BridgeScope();
const decoder = await DecoderRef.bind<string>(
  { decode: async () => "text" },
  { output: types.string, scope, session },
);
assert(
  encodeDecoder(decoder, session, types.string) === "checked-decoder-view",
  "exact decoder projection failed",
);

// Assertions/any can bypass TypeScript, so runtime descriptor checks still have
// to reject a forged associated pin.
const repinned = decoder as unknown as DecoderInput<Invoice>;
await expectReject(
  () => encodeDecoder(repinned, session, types.Invoice),
  /associated pin mismatch: expected Invoice, got string/,
);

const invalidReturn = await DecoderRef.bind<string>(
  { decode: async () => 42 } as unknown as DecoderHost<string>,
  { output: types.string, scope: new BridgeScope(), session: "invalid-return" },
);
await expectReject(() => invalidReturn.decode({ kind: "image" }), /outside associated pin string/);

const agent = new Agent();
const resume = await (
  await agent.run(new FunctionSpec<Resume>("resume", async () => ({ name: "Ada" })))
).get_value();
const invoice = await (
  await agent.run(new FunctionSpec<Invoice>("invoice", () => ({ vendor: "Acme" })))
).get_value();
assert(resume.name === "Ada" && invoice.vendor === "Acme", "per-call Agent output failed");

// Scheduler simulation only: Promise normalization permits immediate/deferred
// callbacks and reentrancy without synchronously blocking the JS event loop.
const order: string[] = [];
async function dispatch<T>(callback: () => T | Promise<T>): Promise<T> {
  return await Promise.resolve().then(callback);
}
await dispatch(async () => {
  order.push("outer:start");
  const nested = await dispatch(() => {
    order.push("inner");
    return "nested";
  });
  await Promise.resolve();
  order.push(`outer:${nested}`);
});
assert(order.join(",") === "outer:start,inner,outer:nested", "reentrant Promise scheduling failed");

// Scope-close simulation only: a pending host Promise observes AbortSignal. A
// real bridge must also retain/drain its call lease and classify cancellation.
const closingScope = new BridgeScope();
const pendingDecoder = await DecoderRef.bind<string>(
  {
    decode(_input, ctx) {
      return new Promise<string>((_resolve, reject) => {
        ctx.signal.addEventListener("abort", () => reject(ctx.signal.reason), { once: true });
      });
    },
  },
  { output: types.string, scope: closingScope, session: "closing" },
);
const pending = pendingDecoder.decode({ kind: "image" });
closingScope.close();
await expectReject(() => pending, /bridge scope closed/);

console.log("runtime probe passed");
console.log("brands are compile-time-only; WeakMap projection checks rejected forged, cross-session, and repinned values");
console.log("scheduler simulation passed: immediate/deferred Promise normalization, reentrancy, and AbortSignal close");
