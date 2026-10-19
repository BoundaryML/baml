// Protocol 3: `startRun` and `startPreviewRun` carry the arguments form's
// JSON text in `args`; protocol 2 sent proto bytes in `argsBytes`.
export const BAML_PLAYGROUND_PROTOCOL_MIN = 3;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 3;

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
