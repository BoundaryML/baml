import {
  Agent,
  BridgeScope,
  ClientRef,
  DecoderRef,
  FunctionSpec,
  GeneratedStringDecoder,
  ResponsesClient,
  acceptClient,
  readText,
  types,
  type BamlType,
  type ClientHost,
  type DecoderHost,
  type DecoderInput,
  type Invoice,
  type MethodInvariantAttempt,
  type PropertyInvariant,
  type Resume,
} from "./contracts.js";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;

const session = "session-a";
const concreteClient = new ResponsesClient(session);
const clientRef = ClientRef.createForProbe(session);

// Direct generated concrete objects and checked refs satisfy the same input role.
acceptClient(concreteClient);
acceptClient(clientRef);

const sameShapedClient: ClientHost = {
  id: () => "host",
};
// @ts-expect-error A host shape is not a bound/generated ClientInput.
acceptClient(sameShapedClient);

const directDecoder = new GeneratedStringDecoder(session);
readText(directDecoder);

const scope = new BridgeScope();
const stringHost: DecoderHost<string> = {
  decode: () => "text",
};

// This is the design document's current two-source inference signature.
declare function bindWithoutNoInfer<Output>(
  implementation: DecoderHost<Output>,
  options: { output: BamlType<Output> },
): Output;

// Both arguments participate in inference, so TS 5.8 reconciles them as the
// wider associated type. That return contract is sound for this producer-only
// Host, but it can accidentally change an associated pin that the host/native
// annotation was meant to select. NoInfer preserves that exact-selection API.
const silentlyWidenedPin = bindWithoutNoInfer(stringHost, { output: types.stringOrNumber });
type _WidenedWithoutNoInfer = Assert<Equal<typeof silentlyWidenedPin, string | number>>;

// @ts-expect-error A method-shaped host contract still needs explicit binding.
readText(stringHost);
const stringDecoder = await DecoderRef.bind(stringHost, {
  output: types.string,
  scope,
  session,
});
readText(stringDecoder);

// Exact generated input pins are invariant.
// @ts-expect-error DecoderInput<Invoice> cannot be repinned to string.
const wrongInputPin: DecoderInput<string> = {} as DecoderInput<Invoice>;
void wrongInputPin;
// @ts-expect-error A string decoder cannot satisfy an Invoice input.
const wrongRefPin: DecoderInput<Invoice> = stringDecoder;
void wrongRefPin;

// NoInfer keeps the host's selected Output from widening to reconcile a token.
// @ts-expect-error Native DecoderHost<string> and Invoice token disagree.
await DecoderRef.bind(stringHost, { output: types.Invoice, scope, session });
// @ts-expect-error Even a wider token is a different exact associated pin.
await DecoderRef.bind(stringHost, { output: types.stringOrNumber, scope, session });

// A method-shaped phantom has a bivariance loophole: this compiles even though
// calling widened.pin(123) could reach an implementation accepting only string.
declare const methodNarrow: MethodInvariantAttempt<string>;
const widenedThroughMethod: MethodInvariantAttempt<string | number> = methodNarrow;
void widenedThroughMethod;

// A function-valued property closes that loophole under strictFunctionTypes.
declare const propertyNarrow: PropertyInvariant<string>;
const propertyObjectLiteral: PropertyInvariant<string> = {
  pin(value) {
    return value;
  },
};
void propertyObjectLiteral;
// @ts-expect-error Function property makes the type parameter invariant.
const cannotWidenProperty: PropertyInvariant<string | number> = propertyNarrow;
void cannotWidenProperty;

const agent = new Agent();
const resumeSpec = new FunctionSpec<Resume>("resume", () => ({ name: "Ada" }));
const invoiceSpec = new FunctionSpec<Invoice>("invoice", () => ({ vendor: "Acme" }));

const resumeResult = await agent.run(resumeSpec);
const resume = await resumeResult.get_value();
type _ResumeInference = Assert<Equal<typeof resume, Resume>>;

const invoiceResult = await agent.run(invoiceSpec);
const invoice = await invoiceResult.get_value();
type _InvoiceInference = Assert<Equal<typeof invoice, Invoice>>;

// @ts-expect-error The same Agent call preserves the spec's per-call output.
const notAnInvoice: Invoice = resume;
void notAnInvoice;

scope.close();
