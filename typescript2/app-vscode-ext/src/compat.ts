export const BAML_LSP_PROTOCOL_MIN = 1;
export const BAML_LSP_PROTOCOL_MAX = 1;
// Mirrors the server's `/api/ws` handshake (`hello` protocol 3 / min
// client 3) and the metadata in `capabilities.experimental.baml`.
// Protocol 3: a run's arguments are the arguments form's JSON text.
export const BAML_PLAYGROUND_PROTOCOL_MIN = 3;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 3;

export type BamlProtocolRange = {
  min: number;
  max: number;
};

export type BamlServerMetadata = {
  toolchainVersion?: string;
  lspProtocol?: number;
  minSupportedClientLspProtocol?: number;
  playgroundProtocol?: number;
  minSupportedClientPlaygroundProtocol?: number;
  capabilities?: string[];
};

export function isProtocolCompatible(
  serverProtocol: number,
  serverMinClientProtocol: number,
  clientRange: BamlProtocolRange,
): boolean {
  return (
    serverProtocol >= clientRange.min &&
    serverProtocol <= clientRange.max &&
    clientRange.max >= serverMinClientProtocol
  );
}
