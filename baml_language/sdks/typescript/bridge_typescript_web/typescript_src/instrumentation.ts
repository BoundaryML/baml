// Keep the private package-root surface aligned with Node. Codegen does not
// expose trace.instrument for browsers or Workers until host recording exists.
export class TraceUsageError extends TypeError {}

type Body = (this: any, ...args: any[]) => any;
type Display = { readonly name?: string };

export function instrument<F extends Body>(body: F): F;
export function instrument<F extends Body>(options: unknown, body: F, display?: Display): F;
export function instrument<F extends Body>(_optionsOrBody: unknown, _body?: F, _display?: Display): F {
  throw new TraceUsageError('Host instrumentation is currently supported by the Node bridge');
}
