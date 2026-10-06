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

type CaptureHandler<T> = (value: T) => unknown;

export function registerCapture<T>(_valueType: new (...args: any[]) => T, _handler: CaptureHandler<T>): CaptureHandler<T> {
  throw new TraceUsageError('Host capture registration is currently supported by the Node bridge');
}

export function captureFor<T>(_valueType: new (...args: any[]) => T): (handler: CaptureHandler<T>) => CaptureHandler<T> {
  throw new TraceUsageError('Host capture registration is currently supported by the Node bridge');
}
