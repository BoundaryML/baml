/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
type CaptureHandler<T = any> = (value: T) => unknown;
export declare function registerCapture<T>(valueType: new (...args: any[]) => T, handler: CaptureHandler<T>): CaptureHandler<T>;
export declare function captureFor<T>(valueType: new (...args: any[]) => T): (handler: CaptureHandler<T>) => CaptureHandler<T>;
export declare function capture(value: unknown): string;
export declare function failureOutcome(error: unknown): 'error' | 'cancelled';
export declare function diagnostic(message: string): void;
export {};
//# sourceMappingURL=host_capture.d.ts.map