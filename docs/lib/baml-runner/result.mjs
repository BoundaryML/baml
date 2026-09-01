import { decodeBase64, decodeOutboundValue } from './outbound.mjs';

const OUTBOUND_RENDERER = 'baml.outbound.base64';

/**
 * Read a successful RunStore outcome across bridge_wasm protocol versions.
 *
 * BAML 0.17 returned a `valueRef`; BAML 0.18 can return the same protobuf
 * inline in `result.value`. Supporting both forms lets the portal and runtime
 * roll forward together without silently rendering a successful run as null.
 */
export async function readRunResult({ boundaryId, outcome, readValue }) {
  if (outcome?.status !== 'succeeded') {
    const message = outcome?.error?.message ?? `run ${outcome?.status ?? 'failed'}`;
    throw new Error(message);
  }

  const result = outcome.result;
  if (!result) return null;

  if (result.rendererHint && result.rendererHint !== OUTBOUND_RENDERER) {
    throw new Error(`unsupported BAML result renderer: ${result.rendererHint}`);
  }

  if (typeof result.value === 'string' && result.value.length > 0) {
    return decodeOutboundValue(decodeBase64(result.value));
  }

  if (result.valueRef) {
    if (typeof readValue !== 'function') {
      throw new Error('the BAML result requires a value reader');
    }
    const body = await readValue(boundaryId, result.valueRef);
    if (!body?.bodyBase64) {
      throw new Error(body?.diagnostic ?? 'the BAML result value is unavailable');
    }
    return decodeOutboundValue(decodeBase64(body.bodyBase64));
  }

  return null;
}
