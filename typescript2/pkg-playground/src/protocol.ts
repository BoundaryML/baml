export const BAML_PLAYGROUND_PROTOCOL_MIN = 1;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 1;

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
