import type { clientInputBrand } from "./internal-brand.js";

export interface HostCallContext {
  readonly signal: AbortSignal;
}

export interface ClientHost {
  id: (ctx: HostCallContext) => string | Promise<string>;
}

export interface ClientInput {
  readonly [clientInputBrand]: true;
}

export function acceptClient(_client: ClientInput): void {}
