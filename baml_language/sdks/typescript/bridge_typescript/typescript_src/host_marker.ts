// Declaration metadata holds no invocation, callback executor, or environment.
import type { BamlHandle } from './native.js';
export type HostMarker = { readonly handle: BamlHandle; readonly identity: object };
const markers = new WeakMap<Function, HostMarker>();
export function hostMarker(function_: Function): HostMarker | undefined { return markers.get(function_); }
export function registerHostMarker(function_: Function, marker: HostMarker): void { markers.set(function_, marker); }
