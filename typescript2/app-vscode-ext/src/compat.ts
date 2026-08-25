export const BAML_LSP_PROTOCOL_MIN = 1;
export const BAML_LSP_PROTOCOL_MAX = 1;
// Mirrors the server's `/api/ws` handshake (`hello` protocol 2 / min
// client 2) and the metadata in `capabilities.experimental.baml`.
export const BAML_PLAYGROUND_PROTOCOL_MIN = 2;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 2;

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
