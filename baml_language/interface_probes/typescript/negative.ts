import {
  BridgeScope,
  DecoderRef,
  acceptClient,
  readText,
  types,
  type ClientHost,
  type DecoderHost,
  type DecoderInput,
  type Invoice,
  type PropertyInvariant,
} from "./contracts.js";

const session = "negative-session";
const scope = new BridgeScope();
const rawHost: ClientHost = { id: () => "raw" };
acceptClient(rawHost);

const stringHost: DecoderHost<string> = { decode: () => "text" };
const stringDecoder = await DecoderRef.bind(stringHost, {
  output: types.string,
  scope,
  session,
});

readText(stringHost);
const invoiceInput: DecoderInput<Invoice> = stringDecoder;
void invoiceInput;

await DecoderRef.bind(stringHost, { output: types.Invoice, scope, session });
await DecoderRef.bind(stringHost, { output: types.stringOrNumber, scope, session });

declare const propertyNarrow: PropertyInvariant<string>;
const propertyWide: PropertyInvariant<string | number> = propertyNarrow;
void propertyWide;
