/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import type { BamlHandle } from './native.js';
export type HostMarker = {
    readonly handle: BamlHandle;
    readonly identity: object;
};
export declare function hostMarker(function_: Function): HostMarker | undefined;
export declare function registerHostMarker(function_: Function, marker: HostMarker): void;
//# sourceMappingURL=host_marker.d.ts.map