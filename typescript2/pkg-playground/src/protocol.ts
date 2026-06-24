export const BAML_PLAYGROUND_PROTOCOL_MIN = 2;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 2;

export function isPlaygroundProtocolCompatible(
  serverProtocol: number,
  minClientProtocol: number,
): boolean {
  return (
    serverProtocol >= BAML_PLAYGROUND_PROTOCOL_MIN &&
    serverProtocol <= BAML_PLAYGROUND_PROTOCOL_MAX &&
    BAML_PLAYGROUND_PROTOCOL_MAX >= minClientProtocol
  );
}
